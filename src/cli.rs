use crate::Error;
use std::net::IpAddr;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "kaspa-miner", about = "A Kaspa high performance CPU miner")]
pub struct Opt {
    #[structopt(short, long, help = "Enable debug logging level")]
    pub debug: bool,
    #[structopt(
        short = "a",
        long = "mining-address",
        help = "The Kaspa address for the miner reward"
    )]
    pub mining_address: String,
    #[structopt(
        short = "s",
        long = "kaspad-address",
        default_value = "127.0.0.1",
        help = "The IP of the kaspad instance"
    )]
    pub kaspad_address: String,

    #[structopt(
        short,
        long,
        help = "Kaspad port (default: Mainnet = 16111, Testnet = 16211)"
    )]
    port: Option<u16>,

    #[structopt(long, help = "Use testnet instead of mainnet (default: false)")]
    testnet: bool,
    #[structopt(
        short = "t",
        long = "threads",
        default_value = "0",
        help = "Amount of miner threads to launch(default: number of logical cpus)"
    )]
    pub num_threads: u16,
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
        println!("kaspad address: {}", self.kaspad_address);
        if self.num_threads == 0 {
            self.num_threads = num_cpus::get_physical().try_into()?;
        }

        Ok(())
    }

    fn port(&mut self) -> u16 {
        *self
            .port
            .get_or_insert_with(|| if self.testnet { 16211 } else { 16110 })
    }
}
