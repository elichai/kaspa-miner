use log::LevelFilter;
use std::{net::IpAddr, str::FromStr};
use structopt::StructOpt;

use crate::Error;

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

    #[structopt(long = "devfund", help = "Mine a percentage of the blocks to the Kaspa devfund [default: Off]")]
    pub devfund_address: Option<String>,

    #[structopt(long = "devfund-percent", help = "The percentage of blocks to send to the devfund", default_value = "1", parse(try_from_str = parse_devfund_percent))]
    pub devfund_percent: u16,

    #[structopt(short, long, help = "Kaspad port [default: Mainnet = 16111, Testnet = 16211]")]
    port: Option<u16>,

    #[structopt(long, help = "Use testnet instead of mainnet [default: false]")]
    testnet: bool,
    #[structopt(
        short = "t",
        long = "threads",
        help = "Amount of miner threads to launch [default: number of logical cpus]"
    )]
    pub num_threads: Option<u16>,
    #[structopt(
        long = "mine-when-not-synced",
        help = "Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-block-when-not-synced` to kaspad  [default: false]"
    )]
    pub mine_when_not_synced: bool,
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
        if self.kaspad_address.is_empty() {
            self.kaspad_address = "127.0.0.1".to_string();
        }

        if !self.kaspad_address.starts_with("grpc://") {
            IpAddr::from_str(&self.kaspad_address)?;
            let port = self.port();
            self.kaspad_address = format!("grpc://{}:{}", self.kaspad_address, port);
        }
        log::info!("kaspad address: {}", self.kaspad_address);

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
