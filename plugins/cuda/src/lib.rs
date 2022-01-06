#[macro_use]
extern crate kaspa_miner;

use std::error::Error as StdError;
use cust::prelude::*;
use log::LevelFilter;
use clap::{ArgMatches,FromArgMatches};
use kaspa_miner::{Worker, Plugin, WorkerSpec};

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

mod cli;
mod worker;

use crate::cli::CudaOpt;
use crate::worker::CudaGPUWorker;

const DEFAULT_WORKLOAD_SCALE : f32= 16.;

pub struct CudaPlugin {
    specs: Vec<CudaWorkerSpec>
}

impl CudaPlugin {
    fn new() -> Self {
        cust::init(CudaFlags::empty()).unwrap();
        env_logger::builder().filter_level(LevelFilter::Info).parse_default_env().init();
        Self{ specs: Vec::new() }
    }
}

impl Plugin for CudaPlugin {
    fn name(&self) -> &'static str {
        "CUDA Worker"
    }

    fn get_worker_specs(&self) -> Vec<Box<dyn WorkerSpec>> {
        self.specs.iter().map(
            |spec| Box::new(spec.clone()) as Box<dyn WorkerSpec>
        ).collect::<Vec<Box<dyn WorkerSpec>>>()
    }

    //noinspection RsTypeCheck
    fn process_option(&mut self, matches: &ArgMatches) -> Result<(), kaspa_miner::Error> {
        let opts = CudaOpt::from_arg_matches(matches)?;
        let gpus : Vec<u16> = match &opts.cuda_device {
            Some(devices) => devices.clone(),
            None => {
                let gpu_count = Device::num_devices().unwrap() as u16;
                (0..gpu_count).collect()
            }
        };

        self.specs = (0..gpus.len()).map(
            |i| CudaWorkerSpec{
                device_id: gpus[i] as u32,
                workload: match &opts.cuda_workload {
                    Some(workload) if workload.len() < i => workload[i],
                    Some(workload) if workload.len() > 0 => *workload.last().unwrap(),
                    _ => DEFAULT_WORKLOAD_SCALE
                },
                is_absolute: opts.cuda_workload_absolute
            }
        ).collect();
        Ok(())
    }
}


#[derive(Copy, Clone)]
struct CudaWorkerSpec {
    device_id: u32,
    workload: f32,
    is_absolute: bool
}

impl WorkerSpec for CudaWorkerSpec {
    fn build(&self) -> Box<dyn Worker> {
        Box::new(CudaGPUWorker::new(self.device_id, self.workload, self.is_absolute).unwrap())
    }
}

declare_plugin!(CudaPlugin, CudaPlugin::new, CudaOpt);