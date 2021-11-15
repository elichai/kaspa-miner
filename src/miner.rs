use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, Error};
use log::info;
use rand::{thread_rng, RngCore};
use std::num::Wrapping;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::{self, error::TryRecvError, Receiver, Sender};
use tokio::task::{self, JoinHandle};
use tokio::time::MissedTickBehavior;

type MinerHandler = std::thread::JoinHandle<Result<(), Error>>;

#[allow(dead_code)]
pub struct MinerManager {
    handles: Vec<MinerHandler>,
    block_channels: Vec<Sender<pow::State>>,
    send_channel: Sender<KaspadMessage>,
    logger_handle: JoinHandle<()>,
}

static HASH_TRIED: AtomicU64 = AtomicU64::new(0);
const LOG_RATE: Duration = Duration::from_secs(10);

impl MinerManager {
    pub fn new(send_channel: Sender<KaspadMessage>, num_threads: u16) -> Self {
        info!("launching: {} miners", num_threads);
        let (handels, channels) = (0..num_threads)
            .map(|_| {
                let (send, recv) = mpsc::channel(1);
                (Self::launch_miner(send_channel.clone(), recv), send)
            })
            .unzip();
        Self {
            handles: handels,
            block_channels: channels,
            send_channel,
            logger_handle: task::spawn(Self::log_hashrate()),
        }
    }

    pub async fn process_block(&mut self, block: RpcBlock) -> Result<(), Error> {
        let state = pow::State::new(block)?;
        for c in &self.block_channels {
            c.send(state.clone()).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn launch_miner(send_channel: Sender<KaspadMessage>, mut block_channel: Receiver<pow::State>) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            let mut state = block_channel.blocking_recv().ok_or("Channel is closed")?;
            loop {
                state.nonce = nonce.0;
                if let Some(block) = state.generate_block_if_pow() {
                    let block_hash =
                        block.block_hash().expect("We just got it from the state, we should be able to hash it");
                    send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                    info!("Found a block: {:x}", block_hash);
                }
                nonce += Wrapping(1);
                // TODO: Is this really necessary? can we just use Relaxed?
                HASH_TRIED.fetch_add(1, Ordering::AcqRel);

                if nonce.0 % 128 == 0 {
                    match block_channel.try_recv() {
                        Ok(new_state) => state = new_state,
                        Err(TryRecvError::Empty) => (),
                        Err(TryRecvError::Disconnected) => return Err(TryRecvError::Disconnected.into()),
                    }
                }
            }
        })
    }

    async fn log_hashrate() {
        let mut ticker = tokio::time::interval(LOG_RATE);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_instant = ticker.tick().await;
        loop {
            let now = ticker.tick().await;
            let hashes = HASH_TRIED.swap(0, Ordering::AcqRel);
            let kilo_hashes = (hashes as f64) / 1000.0;
            let rate = kilo_hashes / (now - last_instant).as_secs_f64();
            info!("Current hashrate is: {:.2} Khash/s", rate);
            last_instant = now;
        }
    }
}
