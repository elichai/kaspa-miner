use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, Error};
use log::{info, warn};
use rand::{thread_rng, RngCore};
use std::num::Wrapping;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, error::TryRecvError, Receiver, Sender};
use tokio::task::{self, JoinHandle};
use tokio::time::MissedTickBehavior;

use cust::prelude::*;
use crate::gpu::GPUContext;

use crate::target::Uint256;

type MinerHandler = std::thread::JoinHandle<Result<(), Error>>;


#[allow(dead_code)]
pub struct MinerManager {
    handles: Vec<MinerHandler>,
    block_channels: Vec<Sender<Option<pow::State>>>,
    send_channel: Sender<KaspadMessage>,
    logger_handle: JoinHandle<()>,
    is_synced: bool,
    hashes_tried: Arc<AtomicU64>,
    //runtime: Arc<Mutex<HashMap<&'static str, u128>>>,
}

impl Drop for MinerManager {
    fn drop(&mut self) {
        self.logger_handle.abort();
    }
}

const LOG_RATE: Duration = Duration::from_secs(10);

impl MinerManager {
    pub fn new(send_channel: Sender<KaspadMessage>, num_threads: u16, gpu_threads: u16, workload: usize) -> Self {
        info!("launching: {} miners", num_threads);
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let (handels, channels) = (0..num_threads)
            .map(|i| {
                let (send, recv) = mpsc::channel(1);
                if i < gpu_threads as u16 {
                    (Self::launch_gpu_miner(send_channel.clone(), recv, Arc::clone(&hashes_tried), i, workload), send)
                } else {
                    (Self::launch_miner(send_channel.clone(), recv, Arc::clone(&hashes_tried)), send)
                }
            })
            .unzip();
        Self {
            handles: handels,
            block_channels: channels,
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

        for c in &self.block_channels {
            c.send(state.clone()).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn launch_gpu_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
        thid: u16,
        workload: usize
    ) -> MinerHandler {
        std::thread::spawn(move | | {
            info!("Spawned GPU Thread #{}", thid);

            let device = Device::get_device(thid as u32)?;
            let _ctx = Context::create_and_push(ContextFlags::MAP_HOST | ContextFlags::SCHED_AUTO, device);
            let gpu_ctx = GPUContext::new(_ctx)?;
            let mut gpu_work = gpu_ctx.get_worker(workload)?;


            let out_size: usize = gpu_work.get_output_size();
            let mut hashes  = vec![[0u8; 32]; out_size];
            let mut nonces= vec![0u64; out_size];

            let mut state = None;

            let mut has_results = false;
            let mut found = false;
            loop{
                // check block header?
                if state.is_none() {
                    state = block_channel.blocking_recv().ok_or(TryRecvError::Disconnected)?;
                } else if nonces[0] % 128 == 0 || found {
                    has_results = false;
                    found = false;
                    match block_channel.try_recv() {
                        Ok(new_state) => state = new_state,
                        Err(TryRecvError::Empty) => (),
                        Err(TryRecvError::Disconnected) => return Err(TryRecvError::Disconnected.into()),
                    }
                }
                let state_ref = match state.as_mut() {
                    Some(s) => s,
                    None => continue,
                };

                gpu_work.sync()?;
                state_ref.start_pow_gpu(&mut gpu_work);

                gpu_work.copy_output_to(&mut hashes, &mut nonces)?;
                if has_results {
                    for i in 1..out_size {
                        if Uint256::from_le_bytes(hashes[i]) <= state_ref.target {
                            if let Some(block) = state_ref.generate_block_if_pow(nonces[i]) {
                                let block_hash =
                                    block.block_hash().expect("We just got it from the state, we should be able to hash it");
                                send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                                info!("Found a block: {:x}", block_hash);
                            } else {
                                warn!("Something is wrong in GPU code!")
                            }
                        }
                    }

                    /*info!("Output should be: {}", state_ref.calculate_pow(nonces[0]).0[3]);
                    info!("We got: {} (Nonces: {})", Uint256::from_le_bytes(hashes[0]).0[3], nonces[0]);
                    if state_ref.calculate_pow(nonces[0]).0[0] != Uint256::from_le_bytes(hashes[0]).0[0] {
                        gpu_work.sync()?;
                        let nonce_vec = vec![nonces[0]; workload];
                        gpu_work.calculate_pow_hash(&state_ref.pow_hash_header, Some(&nonce_vec));
                        gpu_work.sync()?;
                        gpu_work.calculate_matrix_mul(&mut state_ref.matrix.clone().0.as_slice().as_dbuf().unwrap());
                        gpu_work.sync()?;
                        gpu_work.calculate_heavy_hash();
                        gpu_work.sync()?;
                        let mut hashes2  = vec![[0u8; 32]; out_size];
                        let mut nonces2= vec![0u64; out_size];
                        gpu_work.copy_output_to(&mut hashes2, &mut nonces2);
                        assert!(state_ref.calculate_pow(nonces[0]).0[0] == Uint256::from_le_bytes(hashes2[0]).0[0]);
                        assert!(nonces2[0] == nonces[0]);
                        assert!(hashes2[0] == hashes[0]);
                        assert!(false);
                    }*/


                    hashes_tried.fetch_add(workload.try_into().unwrap(), Ordering::AcqRel);
                }

                gpu_work.calculate_heavy_hash();
                has_results = true;


            }
        })
    }


    fn launch_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
    ) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            let mut state = None;
            loop {
                if state.is_none() {
                    state = block_channel.blocking_recv().ok_or(TryRecvError::Disconnected)?;
                }
                let state_ref = match state.as_mut() {
                    Some(s) => s,
                    None => continue,
                };
                if let Some(block) = state_ref.generate_block_if_pow(nonce.0) {
                    let block_hash =
                        block.block_hash().expect("We just got it from the state, we should be able to hash it");
                    send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                    info!("Found a block: {:x}", block_hash);
                }
                nonce += Wrapping(1);
                // TODO: Is this really necessary? can we just use Relaxed?
                hashes_tried.fetch_add(1, Ordering::AcqRel);

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
