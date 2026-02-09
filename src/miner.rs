use crate::{
    pow,
    proto::{KaspadMessage, RpcBlock},
    swap_rust::WatchSwap,
    Error, ShutdownHandler,
};
use log::{info, warn};
use rand::{thread_rng, RngCore};
use std::{
    num::Wrapping,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::mpsc::Sender,
    task::{self, JoinHandle},
    time::MissedTickBehavior,
};

type MinerHandler = std::thread::JoinHandle<Result<(), Error>>;

#[allow(dead_code)]
pub struct MinerManager {
    handles: Vec<MinerHandler>,
    block_channel: WatchSwap<pow::State>,
    send_channel: Sender<KaspadMessage>,
    logger_handle: JoinHandle<()>,
    is_synced: bool,
    hashes_tried: Arc<AtomicU64>,
    current_state_id: AtomicUsize,
}

impl Drop for MinerManager {
    fn drop(&mut self) {
        self.logger_handle.abort();
    }
}

pub fn get_num_cpus(n_cpus: Option<u16>) -> u16 {
    n_cpus.unwrap_or_else(|| {
        num_cpus::get_physical().try_into().expect("Doesn't make sense to have more than 65,536 CPU cores")
    })
}

const LOG_RATE: Duration = Duration::from_secs(10);

impl MinerManager {
    pub fn new(
        send_channel: Sender<KaspadMessage>,
        n_cpus: Option<u16>,
        throttle: Option<Duration>,
        shutdown: ShutdownHandler,
    ) -> Self {
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let watch = WatchSwap::empty();
        let handles = Self::launch_cpu_threads(
            send_channel.clone(),
            hashes_tried.clone(),
            watch.clone(),
            shutdown,
            n_cpus,
            throttle,
        )
        .collect();

        Self {
            handles,
            block_channel: watch,
            send_channel,
            logger_handle: task::spawn(Self::log_hashrate(Arc::clone(&hashes_tried))),
            is_synced: true,
            hashes_tried,
            current_state_id: AtomicUsize::new(0),
        }
    }

    fn launch_cpu_threads(
        send_channel: Sender<KaspadMessage>,
        hashes_tried: Arc<AtomicU64>,
        work_channel: WatchSwap<pow::State>,
        shutdown: ShutdownHandler,
        n_cpus: Option<u16>,
        throttle: Option<Duration>,
    ) -> impl Iterator<Item = MinerHandler> {
        let n_cpus = get_num_cpus(n_cpus);
        info!("Launching: {} cpu miners", n_cpus);
        (0..n_cpus).map(move |_| {
            Self::launch_cpu_miner(
                send_channel.clone(),
                work_channel.clone(),
                hashes_tried.clone(),
                throttle,
                shutdown.clone(),
            )
        })
    }

    pub fn process_block(&mut self, block: Option<RpcBlock>) -> Result<(), Error> {
        let state = if let Some(b) = block {
            self.is_synced = true;
            // Relaxed ordering here means there's no promise that the counter will always go up, but the id will always be unique
            let id = self.current_state_id.fetch_add(1, Ordering::Relaxed);
            Some(pow::State::new(id, b)?)
        } else {
            if !self.is_synced {
                return Ok(());
            }
            self.is_synced = false;
            warn!("Kaspad is not synced, skipping current template");
            None
        };

        self.block_channel.swap(state);
        Ok(())
    }

    pub fn launch_cpu_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: WatchSwap<pow::State>,
        hashes_tried: Arc<AtomicU64>,
        throttle: Option<Duration>,
        shutdown: ShutdownHandler,
    ) -> MinerHandler {
        // We mark it cold as the function is not called often, and it's not in the hot path
        #[cold]
        fn found_block(send_channel: &Sender<KaspadMessage>, block: RpcBlock) -> Result<(), Error> {
            let block_hash = block.block_hash().expect("We just got it from the state, we should be able to hash it");
            send_channel.blocking_send(KaspadMessage::submit_block(block))?;
            info!("Found a block: {:x}", block_hash);
            Ok(())
        }

        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            let mut state = None;
            loop {
                if state.is_none() {
                    state = block_channel.wait_for_change().as_deref().cloned();
                }
                let Some(state_ref) = state.as_mut() else {
                    continue;
                };
                state_ref.nonce = nonce.0;

                if let Some(block) = state_ref.generate_block_if_pow() {
                    found_block(&send_channel, block)?;
                }
                nonce += Wrapping(1);

                if nonce.0.is_multiple_of(128) {
                    hashes_tried.fetch_add(128, Ordering::Relaxed);
                    if shutdown.is_shutdown() {
                        return Ok(());
                    }
                    if let Some(new_state) = block_channel.get_changed() {
                        state = new_state.as_deref().cloned();
                    }
                }

                if let Some(sleep_duration) = throttle {
                    std::thread::sleep(sleep_duration)
                }
            }
        })
    }

    async fn log_hashrate(hashes_tried: Arc<AtomicU64>) {
        let mut ticker = tokio::time::interval(LOG_RATE);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_instant = ticker.tick().await;
        for i in 0u64.. {
            let now = ticker.tick().await;
            let hashes = hashes_tried.swap(0, Ordering::Relaxed);
            let rate = (hashes as f64) / (now - last_instant).as_secs_f64();
            if hashes == 0 && i % 2 == 0 {
                warn!("Kaspad is still not synced");
            } else if hashes != 0 {
                let (rate, suffix) = Self::hash_suffix(rate);
                info!("Current hashrate is: {:.2} {}", rate, suffix);
            }
            last_instant = now;
        }
    }

    #[inline]
    fn hash_suffix(n: f64) -> (f64, &'static str) {
        match n {
            n if n < 1_000.0 => (n, "hash/s"),
            n if n < 1_000_000.0 => (n / 1_000.0, "Khash/s"),
            n if n < 1_000_000_000.0 => (n / 1_000_000.0, "Mhash/s"),
            n if n < 1_000_000_000_000.0 => (n / 1_000_000_000.0, "Ghash/s"),
            n if n < 1_000_000_000_000_000.0 => (n / 1_000_000_000_000.0, "Thash/s"),
            _ => (n, "hash/s"),
        }
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;

    use self::test::{black_box, Bencher};
    use crate::pow::State;
    use crate::proto::{RpcBlock, RpcBlockHeader};
    use rand::{thread_rng, RngCore};

    #[bench]
    pub fn bench_mining(bh: &mut Bencher) {
        let mut state = State::new(
            1,
            RpcBlock {
                header: Some(RpcBlockHeader {
                    version: 1,
                    parents: vec![],
                    hash_merkle_root: "23618af45051560529440541e7dc56be27676d278b1e00324b048d410a19d764".to_string(),
                    accepted_id_merkle_root: "947d1a10378d6478b6957a0ed71866812dee33684968031b1cace4908c149d94"
                        .to_string(),
                    utxo_commitment: "ec5e8fc0bc0c637004cee262cef12e7cf6d9cd7772513dbd466176a07ab7c4f4".to_string(),
                    timestamp: 654654353,
                    bits: 0x1e7fffff,
                    nonce: 0,
                    daa_score: 654456,
                    blue_work: "d8e28a03234786".to_string(),
                    pruning_point: "be4c415d378f9113fabd3c09fcc84ddb6a00f900c87cb6a1186993ddc3014e2d".to_string(),
                    blue_score: 1164419,
                }),
                transactions: vec![],
                verbose_data: None,
            },
        )
        .unwrap();
        state.nonce = thread_rng().next_u64();
        bh.iter(|| {
            for _ in 0..100 {
                black_box(state.check_pow());
                state.nonce += 1;
            }
        });
    }
}
