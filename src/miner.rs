use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, Error};
use log::{info, warn};
use rand::{thread_rng, RngCore};
use std::num::Wrapping;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
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
}

impl Drop for MinerManager {
    fn drop(&mut self) {
        self.logger_handle.abort();
    }
}

const LOG_RATE: Duration = Duration::from_secs(10);

impl MinerManager {
    pub fn new(send_channel: Sender<KaspadMessage>, num_threads: u16) -> Self {
        info!("launching: {} miners", num_threads);
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let (send, recv) = watch::channel(None);
        let handels = (0..num_threads)
            .map(|_| Self::launch_miner(send_channel.clone(), recv.clone(), Arc::clone(&hashes_tried)))
            .collect();
        Self {
            handles: handels,
            block_channel: send,
            send_channel,
            logger_handle: task::spawn(Self::log_hashrate(Arc::clone(&hashes_tried))),
            is_synced: true,
            hashes_tried,
        }
    }

    pub async fn process_block(&mut self, block: Option<RpcBlock>) -> Result<(), Error> {
        let state = match block {
            Some(b) => {
                self.is_synced = true;
                Some(pow::State::new(b)?)
            }
            None => {
                if !self.is_synced {
                    return Ok(());
                }
                self.is_synced = false;
                warn!("Kaspad is not synced, skipping current template");
                None
            }
        };

        if !&self.block_channel.is_closed() {
            self.block_channel.send(state.clone()).map_err(|_e| "Failed sending block to threads")?;
        }
        Ok(())
    }

    fn launch_miner(
        send_channel: Sender<KaspadMessage>,
        block_channel: watch::Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
    ) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            let mut state = None;
            loop {
                while state.is_none() {
                    sleep(Duration::from_millis(500));
                    state = block_channel.borrow().clone();
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
                    if (&block_channel).borrow().is_none()
                        || ((&block_channel).borrow().as_ref().unwrap().id != state.as_ref().unwrap().id)
                    {
                        state = (&block_channel).borrow().clone();
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
            let kilo_hashes = (hashes as f64) / 1000.0;
            let rate = kilo_hashes / (now - last_instant).as_secs_f64();
            if hashes == 0 && i % 2 == 0 {
                warn!("Kaspad is still not synced")
            } else if hashes != 0 {
                info!("Current hashrate is: {:.2} Khash/s", rate);
            }
            last_instant = now;
        }
    }
}
