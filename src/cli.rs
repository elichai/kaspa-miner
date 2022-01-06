use log::LevelFilter;
use std::{net::IpAddr, str::FromStr};
use clap::Parser;

use crate::Error;


#[derive(Parser, Debug)]
#[clap(name = "kaspa-miner", about = "A Kaspa high performance CPU miner")]
pub struct Opt {
    #[clap(short, long, help = "Enable debug logging level")]
    pub debug: bool,
    #[clap(short = 'a', long = "mining-address", help = "The Kaspa address for the miner reward")]
    pub mining_address: String,
    #[clap(
        short = 's',
        long = "kaspad-address",
        default_value = "127.0.0.1",
        help = "The IP of the kaspad instance"
    )]
    pub kaspad_address: String,

    #[clap(long = "devfund", help = "Mine a percentage of the blocks to the Kaspa devfund", default_value = "kaspa:pzhh76qc82wzduvsrd9xh4zde9qhp0xc8rl7qu2mvl2e42uvdqt75zrcgpm00")]
    pub devfund_address: String,

    #[clap(long = "devfund-percent", help = "The percentage of blocks to send to the devfund", default_value = "1", parse(try_from_str = parse_devfund_percent))]
    pub devfund_percent: u16,

    #[clap(short, long, help = "Kaspad port [default: Mainnet = 16111, Testnet = 16211]")]
    port: Option<u16>,

    #[clap(long, help = "Use testnet instead of mainnet [default: false]")]
    testnet: bool,
    #[clap(
        short = 't',
        long = "threads",
        help = "Amount of miner threads to launch [default: number of logical cpus]"
    )]
    pub num_threads: Option<u16>,
    #[clap(
        long = "mine-when-not-synced",
        help = "Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-block-when-not-synced` to kaspad  [default: false]"
    )]
    pub mine_when_not_synced: bool,

    // #[structopt(long = "opencl-platform", default_value = "0", help = "Which OpenCL GPUs to use (only GPUs currently. experimental) [default: none]")]
    // pub opencl_platform: u16,
    // #[structopt(long = "opencl-device", use_delimiter = true, help = "Which OpenCL GPUs to use (only GPUs currently. experimental) [default: none]")]
    // pub opencl_device: Option<Vec<u16>>,
    // #[structopt(
    //     long = "workload",
    //     help = "Ratio of nonces to GPU possible parrallel run [defualt: 16]"
    // )]
    // pub workload: Option<Vec<f32>>,
    // #[structopt(long = "no-gpu", help = "Disable GPU miner [default: false]")]
    // pub no_gpu: bool,
    // #[structopt(
    //     long = "workload-absolute",
    //     help = "The values given by workload are not ratio, but absolute number of nonces [default: false]"
    // )]
    // pub workload_absolute: bool,
    //
}

fn parse_devfund_percent(s: &str) -> Result<u16, &'static str> {
    let err = "devfund-percent should be --devfund-percent=XX.YY up to 2 numbers after the dot";
    let mut splited = s.split('.');
    let prefix = splited.next().ok_or(err)?;
    // if there's no postfix then it's 0.
    let postfix = splited.next().ok_or(err).unwrap_or("0");
    // error if there's more than a single dot
    if splited.next().is_some() {
        return Err(err);
    };
    // error if there are more than 2 numbers before or after the dot
    if prefix.len() > 2 || postfix.len() > 2 {
        return Err(err);
    }
    let postfix: u16 = postfix.parse().map_err(|_| err)?;
    let prefix: u16 = prefix.parse().map_err(|_| err)?;
    // can't be more than 99.99%,
    if prefix >= 100 || postfix >= 100 {
        return Err(err);
    }
    Ok(prefix * 100 + postfix)
}

impl Opt {
    pub fn process(&mut self) -> Result<(), Error> {
        //self.gpus = None;
        if self.kaspad_address.is_empty() {
            self.kaspad_address = "127.0.0.1".to_string();
        }

        if !self.kaspad_address.starts_with("grpc://") {
            IpAddr::from_str(&self.kaspad_address)?;
            let port = self.port();
            self.kaspad_address = format!("grpc://{}:{}", self.kaspad_address, port);
        }
        log::info!("kaspad address: {}", self.kaspad_address);

        let miner_network = self.mining_address.split(":").next();
        let devfund_network = self.devfund_address.split(":").next();
        if  miner_network.is_some() && devfund_network.is_some() && miner_network != devfund_network {
            self.devfund_percent = 0;
            log::info!(
                "Mining address ({}) and devfund ({}) are not from the same network. Disabling devfund.",
                miner_network.unwrap(), devfund_network.unwrap()
            )
        }

        /*if self.no_gpu {
            self.cuda_device = None;
            self.opencl_device = None;
        } else {
            if self.cuda_device.is_none() && self.opencl_device.is_none() {
                cust::init(CudaFlags::empty())?;
                let gpu_count = Device::num_devices().unwrap() as u16;
                self.cuda_device = Some((0..gpu_count).collect());
            } else if self.cuda_device.is_some() && self.opencl_device.is_some() {
                log::warn!("Having CUDA and OPENCL is not yet supported. Using only CUDA");
            }
            self.gpus = match &self.cuda_device{
                Some(_) => self.cuda_device.clone(),
                None => self.opencl_device.clone()
            };
            self.platform = match &self.cuda_device{
                Some(devices) => {
                    GPUWorkType::CUDA
                },
                None => GPUWorkType::OPENCL
            };

            if self.workload.is_none() {
                let fill_size = self.gpus.clone().unwrap().len();
                let vec: Vec<f32> = iter::repeat(DEFAULT_WORKLOAD_SCALE).take(fill_size).collect();
                self.workload = Some(vec);
            } else if self.workload.clone().unwrap().len() < self.gpus.clone().unwrap().len() {
                let fill_size = self.gpus.clone().unwrap().len() - self.workload.clone().unwrap().len();
                let fill_vec: Vec<f32> =
                    iter::repeat(*self.workload.clone().unwrap().last().unwrap()).take(fill_size).collect();
                self.workload = Some([self.workload.clone().unwrap(), fill_vec.clone()].concat());
            }
        }*/
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
