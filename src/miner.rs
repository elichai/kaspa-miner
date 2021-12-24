use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, Error};
use log::{error, info, warn};
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

use crate::gpu::GPUWork;
use crate::target::Uint256;

type MinerHandler = std::thread::JoinHandle<Result<(), Error>>;

#[allow(dead_code)]
pub struct MinerManager {
    handles: Vec<MinerHandler>,
    block_channel: watch::Sender<Option<pow::State>>,
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
    pub fn new(
        send_channel: Sender<KaspadMessage>,
        num_threads: usize,
        gpus: Vec<u16>,
        workload: Vec<f32>,
        workload_absolute: bool,
    ) -> Self {
        info!("launching: {} miners", num_threads);
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let (send, recv) = watch::channel(None);
        let handels = (0..(num_threads + gpus.len()))
            .map(|i| {
                if i < gpus.len() {
                    Self::launch_gpu_miner(
                        send_channel.clone(),
                        recv.clone(),
                        Arc::clone(&hashes_tried),
                        gpus.clone(),
                        i,
                        workload.clone(),
                        workload_absolute,
                    )
                } else {
                    Self::launch_miner(send_channel.clone(), recv.clone(), Arc::clone(&hashes_tried))
                }
            })
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
                Some(pow::State::new(b).unwrap())
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
            self.block_channel.send(state.clone());
        }
        Ok(())
    }

    fn launch_gpu_miner(
        send_channel: Sender<KaspadMessage>,
        block_channel: watch::Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
        gpus: Vec<u16>,
        thd_id: usize,
        workload: Vec<f32>,
        workload_absolute: bool,
    ) -> MinerHandler {
        std::thread::spawn(move || {
            (|| {
                info!("Spawned Thread for GPU #{}", gpus[thd_id]);
                let mut gpu_work = GPUWork::new(gpus[thd_id] as u32, workload[thd_id], workload_absolute)?;
                //let mut gpu_work = gpu_ctx.get_worker(workload).unwrap();
                let out_size: usize = gpu_work.get_output_size();

                let mut hashes = vec![[0u8; 32]; out_size];
                let mut nonces = vec![0u64; out_size];

                let mut state = None;

                let mut has_results = false;
                loop {
                    while state.is_none() {
                        sleep(Duration::from_millis(500));
                        state = block_channel.borrow().clone();
                    }
                    let state_ref = match state.as_mut() {
                        Some(s) => s,
                        None => continue,
                    };

                    gpu_work.sync().unwrap();

                    state_ref.start_pow_gpu(&mut gpu_work);
                    gpu_work.copy_output_to(&mut hashes, &mut nonces)?;

                    if has_results {
                        for i in 0..gpu_work.get_output_size() {
                            if Uint256::from_le_bytes(hashes[i]) <= state_ref.target {
                                if let Some(block) = state_ref.generate_block_if_pow(nonces[i]) {
                                    let block_hash = block
                                        .block_hash()
                                        .expect("We just got it from the state, we should be able to hash it");
                                    send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                                    info!("Found a block: {:x}", block_hash);
                                    break;
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

                        hashes_tried.fetch_add(gpu_work.workload.try_into().unwrap(), Ordering::AcqRel);
                    }

                    gpu_work.calculate_heavy_hash();
                    has_results = true;
                    {
                        if block_channel.borrow().is_none()
                            || (&block_channel).borrow().as_ref().unwrap().id != state.as_ref().unwrap().id
                        {
                            state = (&block_channel).borrow().clone();
                            has_results = false;
                        }
                    }
                }
                Ok(())
            })()
            .map_err(|e: Error| {
                error!("GPU thread of #{} crashed: {}", thd_id, e.to_string());
                e
            })
        })
    }

    fn launch_miner(
        send_channel: Sender<KaspadMessage>,
        block_channel: watch::Receiver<Option<pow::State>>,
        hashes_tried: Arc<AtomicU64>,
    ) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            (|| {
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
                        if (&block_channel).borrow().is_none()
                            || ((&block_channel).borrow().as_ref().unwrap().id != state.as_ref().unwrap().id)
                        {
                            state = (&block_channel).borrow().clone();
                        }
                    }
                }
                Ok(())
            })()
            .map_err(|e: Error| {
                error!("CPU thread crashed: {}", e.to_string());
                e
            })
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
