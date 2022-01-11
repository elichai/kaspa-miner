#[macro_use]
extern crate kaspa_miner;

use std::error::Error as StdError;
use log::LevelFilter;
use clap::{ArgMatches,FromArgMatches};
use opencl3::device::{CL_DEVICE_TYPE_ALL, Device};
use opencl3::platform::{get_platforms, Platform};
use opencl3::types::cl_device_id;
use kaspa_miner::{Worker, Plugin, WorkerSpec};

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

mod cli;
mod worker;

use crate::cli::{NonceGenEnum, OpenCLOpt};
use crate::worker::OpenCLGPUWorker;

const DEFAULT_WORKLOAD_SCALE : f32= 512.;

pub struct OpenCLPlugin {
    specs: Vec<OpenCLWorkerSpec>,
    _enabled: bool,
}

impl OpenCLPlugin {
    fn new() -> Result<Self, Error> {
        env_logger::builder().filter_level(LevelFilter::Info).parse_default_env().init();
        Ok(Self{ specs: Vec::new(), _enabled: false })
    }
}

impl Plugin for OpenCLPlugin {
    fn name(&self) -> &'static str {
        "OpenCL Worker"
    }

    fn enabled(&self) -> bool {
        self._enabled
    }

    fn get_worker_specs(&self) -> Vec<Box<dyn WorkerSpec>> {
        self.specs.iter().map(
            |spec| Box::new(spec.clone()) as Box<dyn WorkerSpec>
        ).collect::<Vec<Box<dyn WorkerSpec>>>()
    }

    //noinspection RsTypeCheck
    fn process_option(&mut self, matches: &ArgMatches) -> Result<(), kaspa_miner::Error> {
        let opts: OpenCLOpt = OpenCLOpt::from_arg_matches(matches)?;

        self._enabled = opts.opencl_enable;

        let platforms = get_platforms().expect("opencl: could not find any platforms");
        let platform: Platform = match opts.opencl_platform {
            Some(idx) => {
                self._enabled = true;
                platforms[idx as usize]
            },
            None => platforms[0]
        };

        let device_ids = platform.get_devices(CL_DEVICE_TYPE_ALL).unwrap();
        let gpus = match opts.opencl_device {
            Some(dev) => {
                self._enabled = true;
                dev.iter().map(|d| device_ids[*d as usize]).collect::<Vec<cl_device_id>>()
            },
            None => device_ids.clone()
        };

        self.specs = (0..gpus.len()).map(
            |i| OpenCLWorkerSpec{
                platform,
                device_id: Device::new(gpus[i]),
                workload: match &opts.opencl_workload {
                    Some(workload) if i < workload.len() => workload[i],
                    Some(workload) if workload.len() > 0 => *workload.last().unwrap(),
                    _ => DEFAULT_WORKLOAD_SCALE
                },
                is_absolute: opts.opencl_workload_absolute,
                experimental_amd: opts.experimental_amd,
                random: opts.nonce_gen
            }
        ).collect();

        Ok(())
    }
}


#[derive(Copy, Clone)]
struct OpenCLWorkerSpec {
    platform: Platform,
    device_id: Device,
    workload: f32,
    is_absolute: bool,
    experimental_amd: bool,
    random: NonceGenEnum,
}

impl WorkerSpec for OpenCLWorkerSpec {
    fn build(&self) -> Box<dyn Worker> {
        Box::new(OpenCLGPUWorker::new(self.device_id, self.workload, self.is_absolute, self.experimental_amd, &self.random).unwrap())
    }
}

declare_plugin!(OpenCLPlugin, OpenCLPlugin::new, OpenCLOpt);