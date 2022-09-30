use std::num::Wrapping;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, watch, Error};
use log::{info, warn};
use rand::{thread_rng, RngCore};
use tokio::sync::mpsc::Sender;
use tokio::task::{self, JoinHandle};
use tokio::time::MissedTickBehavior;

type MinerHandler = std::thread::JoinHandle<Result<(), Error>>;

#[allow(dead_code)]
pub struct MinerManager {
    handles: Vec<MinerHandler>,
    block_channel: watch::Sender<Option<pow::State>>,
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
    pub fn new(send_channel: Sender<KaspadMessage>, n_cpus: Option<u16>) -> Self {
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let (send, recv) = watch::channel(None);
        let handles = Self::launch_cpu_threads(send_channel.clone(), Arc::clone(&hashes_tried), recv, n_cpus).collect();
        Self {
            handles,
            block_channel: send,
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
        work_channel: watch::Receiver<Option<pow::State>>,
        n_cpus: Option<u16>,
    ) -> impl Iterator<Item = MinerHandler> {
        let n_cpus = get_num_cpus(n_cpus);
        info!("launching: {} cpu miners", n_cpus);
        (0..n_cpus)
            .map(move |_| Self::launch_cpu_miner(send_channel.clone(), work_channel.clone(), Arc::clone(&hashes_tried)))
    }

    pub fn process_block(&mut self, block: Option<RpcBlock>) -> Result<(), Error> {
        let state = if let Some(b) = block {
            self.is_synced = true;
            let id = self.current_state_id.fetch_add(1, Ordering::SeqCst);
            Some(pow::State::new(id, b)?)
        } else {
            if !self.is_synced {
                return Ok(());
            }
            self.is_synced = false;
            warn!("Kaspad is not synced, skipping current template");
            None
        };

        self.block_channel.send(state).map_err(|_e| "Failed sending block to threads")?;
        Ok(())
    }

    pub fn launch_cpu_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: watch::Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
    ) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            let mut state = None;
            loop {
                if state.is_none() {
                    state = block_channel.wait_for_change()?;
                }
                let state_ref = match state.as_mut() {
                    Some(s) => s,
                    None => continue,
                };
                state_ref.nonce = nonce.0;
                if let Some(block) = state_ref.generate_block_if_pow() {
                    let block_hash =
                        block.block_hash().expect("We just got it from the state, we should be able to hash it");
                    send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                    info!("Found a block: {:x}", block_hash);
                }
                nonce += Wrapping(1);
                // TODO: Is this really necessary? can we just use Relaxed?
                hashes_tried.fetch_add(1, Ordering::AcqRel);

                if nonce.0 % 128 == 0 {
                    if let Some(new_state) = block_channel.get_changed()? {
                        state = new_state;
                    }
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
            let hashes = hashes_tried.swap(0, Ordering::AcqRel);
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
        let mut state = State::new(RpcBlock {
            header: Some(RpcBlockHeader {
                version: 1,
                parents: vec![],
                hash_merkle_root: "23618af45051560529440541e7dc56be27676d278b1e00324b048d410a19d764".to_string(),
                accepted_id_merkle_root: "947d1a10378d6478b6957a0ed71866812dee33684968031b1cace4908c149d94".to_string(),
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
        })
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
