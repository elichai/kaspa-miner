use cust::device::Device;
use log::LevelFilter;
use std::cmp::max;
use std::{iter, net::IpAddr, str::FromStr};
use structopt::StructOpt;

use crate::Error;

const DEFAULT_WORKLOAD_SCALE: f32 = 32.;

#[derive(Debug, StructOpt)]
#[structopt(name = "kaspa-miner", about = "A Kaspa high performance CPU miner")]
pub struct Opt {
    #[structopt(short, long, help = "Enable debug logging level")]
    pub debug: bool,
    #[structopt(short = "a", long = "mining-address", help = "The Kaspa address for the miner reward")]
    pub mining_address: String,
    #[structopt(
        short = "s",
        long = "kaspad-address",
        default_value = "127.0.0.1",
        help = "The IP of the kaspad instance"
    )]
    pub kaspad_address: String,

    #[structopt(short, long, help = "Kaspad port [default: Mainnet = 16111, Testnet = 16211]")]
    port: Option<u16>,

    #[structopt(long, help = "Use testnet instead of mainnet [default: false]")]
    testnet: bool,
    #[structopt(
        short = "t",
        long = "threads",
        help = "Amount of CPU miner threads to launch. The first thread manages the GPU, if not disabled [default: number of logical cpus minus number of gpu]"
    )]
    pub num_threads: Option<usize>,
    #[structopt(
        long = "mine-when_not-synced",
        help = "Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-block-when-not-synced` to kaspad  [default: false]"
    )]
    pub mine_when_not_synced: bool,
    #[structopt(long = "cuda-device", help = "Which GPUs to use [default: all]")]
    pub cuda_device: Option<Vec<u16>>,
    #[structopt(
        long = "workload",
        help = "Ratio of nonces to GPU possible parrallel run [defualt: {DEFAULT_WORKLOAD_SCALE}]"
    )]
    pub workload: Option<Vec<f32>>,
    #[structopt(long = "no-gpu", help = "Disable GPU miner [default: false]")]
    pub no_gpu: bool,
    #[structopt(
        long = "workload-absolute",
        help = "The values given by workload are not ratio, but absolute number of nonces [default: false]"
    )]
    pub workload_absolute: bool,
}

impl Opt {
    pub fn process(&mut self) -> Result<(), Error> {
        if self.kaspad_address.is_empty() {
            self.kaspad_address = "127.0.0.1".to_string();
        }

        if !self.kaspad_address.starts_with("grpc://") {
            IpAddr::from_str(&self.kaspad_address)?;
            let port = self.port();
            self.kaspad_address = format!("grpc://{}:{}", self.kaspad_address, port);
        }

        let gpu_count = Device::num_devices().unwrap() as u16;
        if self.cuda_device.is_none() {
            self.cuda_device = Some((0..gpu_count).collect());
        }

        if self.workload.is_none() {
            let fill_size = self.cuda_device.clone().unwrap().len();
            let vec: Vec<f32> = iter::repeat(DEFAULT_WORKLOAD_SCALE).take(fill_size).collect();
            self.workload = Some(vec);
        } else if self.workload.clone().unwrap().len() < self.cuda_device.clone().unwrap().len() {
            let fill_size = self.cuda_device.clone().unwrap().len() - self.workload.clone().unwrap().len();
            let fill_vec: Vec<f32> =
                iter::repeat(*self.workload.clone().unwrap().last().unwrap()).take(fill_size).collect();
            self.workload = Some([self.workload.clone().unwrap(), fill_vec.clone()].concat());
        }

        if self.num_threads.is_none() {
            self.num_threads = Some(max(
                num_cpus::get_physical() - self.cuda_device.clone().or_else(|| Some(vec![0u16; 0])).unwrap().len(),
                0,
            ));
        }

        Ok(())
    }

    fn port(&mut self) -> u16 {
        *self.port.get_or_insert_with(|| if self.testnet { 16211 } else { 16110 })
    }

    pub fn log_level(&self) -> LevelFilter {
        if self.debug {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        }
    }
}
