use crate::proto::{KaspadMessage, RpcBlock};
use crate::{pow, Error};
use log::{error, info, warn};
use rand::{thread_rng, RngCore};
use std::num::Wrapping;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{self, error::TryRecvError, Receiver, Sender};
use tokio::task::{self, JoinHandle};
use tokio::time::MissedTickBehavior;

use crate::gpu::GPUWork;

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
    is_submitting: Arc<(Mutex<bool>, Condvar)>,
    //runtime: Arc<Mutex<HashMap<&'static str, u128>>>,
}

impl Drop for MinerManager {
    fn drop(&mut self) {
        self.logger_handle.abort();
    }
}

const LOG_RATE: Duration = Duration::from_secs(10);

impl MinerManager {
    pub fn new(send_channel: Sender<KaspadMessage>, num_threads: usize, gpus: Vec<u16>, workload: Option<Vec<usize>>) -> Self {
        info!("launching: {} miners", num_threads);
        let hashes_tried = Arc::new(AtomicU64::new(0));
        let is_submitting = Arc::new((Mutex::new(false), Condvar::new()));
        let (handels, channels) = (0..(num_threads + gpus.len()))
            .map(|i| {
                let (send, recv) = mpsc::channel(1);
                if i < gpus.len() {
                    (Self::launch_gpu_miner(send_channel.clone(), recv, Arc::clone(&is_submitting),Arc::clone(&hashes_tried), gpus.clone(), i, workload.clone()), send)
                } else {
                    (Self::launch_miner(send_channel.clone(), recv, Arc::clone(&is_submitting), Arc::clone(&hashes_tried)), send)
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
            is_submitting
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

        for c in &self.block_channels {
            if !c.is_closed() {
                c.send(state.clone()).await.unwrap_or_else(|e| {
                    error!("error sending block to thread: {}", e.to_string());
                });
            }
        }
        Ok(())
    }

    pub fn notify_submission(&self){
        let (lock, cond) = &*self.is_submitting;
        let mut submission_indicator = lock.lock().unwrap();
        *submission_indicator = false;
        cond.notify_all();
    }

    fn launch_gpu_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: Receiver<Option<pow::State>>,
        is_submitting: Arc<(Mutex<bool>, Condvar)>,
        hashes_tried: Arc<AtomicU64>,
        gpus: Vec<u16>,
        thd_id: usize,
        workload: Option<Vec<usize>>
    ) -> MinerHandler {
        std::thread::spawn(move || {
            (| |{
                info!("Spawned Thread for GPU #{}", gpus[thd_id]);
                let mut gpu_work = GPUWork::new(gpus[thd_id] as u32, workload.clone().and_then(|w| Some(w[thd_id])).or_else(|| None))?;
                //let mut gpu_work = gpu_ctx.get_worker(workload).unwrap();
                let out_size: usize = gpu_work.get_output_size();

                let mut hashes = vec![[0u8; 32]; out_size];
                let mut nonces = vec![0u64; out_size];

                let mut state = None;

                let mut has_results = false;
                let mut found = false;
                let (lock, cond) = &*is_submitting;
                loop{
                    {
                        let _guard = cond.wait_timeout_while(lock.lock().unwrap(), Duration::from_millis(100), |submission_indicator| { *submission_indicator }).unwrap();
                    }
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

                    gpu_work.sync().unwrap();

                    state_ref.start_pow_gpu(&mut gpu_work);
                    gpu_work.copy_output_to(&mut hashes, &mut nonces)?;

                    if has_results {
                        for i in 0..gpu_work.get_output_size() {
                            if Uint256::from_le_bytes(hashes[i]) <= state_ref.target {
                                if let Some(block) = state_ref.generate_block_if_pow(nonces[i]) {
                                    let block_hash =
                                        block.block_hash().expect("We just got it from the state, we should be able to hash it");
                                    {
                                        let mut submission_indicator = lock.lock().unwrap();
                                        *submission_indicator = true;
                                        send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                                    }
                                    info!("Found a block: {:x}", block_hash);
                                    found = true;
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


                }
            })().map_err(|e: Error| {
                error!("GPU thread of #{} crashed: {}", thd_id, e.to_string());
                e
            })
        })
    }


    fn launch_miner(
        send_channel: Sender<KaspadMessage>,
        mut block_channel: Receiver<Option<pow::State>>,
        is_submitting: Arc<(Mutex<bool>, Condvar)>,
        hashes_tried: Arc<AtomicU64>,
    ) -> MinerHandler {
        let mut nonce = Wrapping(thread_rng().next_u64());
        std::thread::spawn(move || {
            (|| {
                let mut state = None;
                let (lock, cond) = &*is_submitting;
                loop {
                    {
                        let _guard = cond.wait_timeout_while(lock.lock().unwrap(), Duration::from_millis(100), |submission_indicator| { *submission_indicator }).unwrap();
                    }
                    if state.is_none() {
                        state = block_channel.blocking_recv().ok_or(TryRecvError::Disconnected).unwrap();
                    }
                    let state_ref = match state.as_mut() {
                        Some(s) => s,
                        None => continue,
                    };
                    if let Some(block) = state_ref.generate_block_if_pow(nonce.0) {
                        let block_hash =
                            block.block_hash().expect("We just got it from the state, we should be able to hash it");
                        {
                            let mut submission_indicator = lock.lock().unwrap();
                            *submission_indicator = true;
                            send_channel.blocking_send(KaspadMessage::submit_block(block))?;
                        }
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
            })().map_err(|e: Error| {
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
