#![cfg_attr(all(test, feature = "bench"), feature(test))]

use clap::Parser;
use log::{info, warn};
use std::error::Error as StdError;
use std::time::Duration;
use chrono::Local;
use std::io::Write;

use crate::cli::Opt;
use crate::client::KaspadHandler;
use crate::miner::MinerManager;
use crate::proto::NotifyBlockAddedRequestMessage;
use crate::target::Uint256;

mod cli;
mod client;
mod kaspad_messages;
mod miner;
mod pow;
mod target;
mod watch;

pub mod proto {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("protowire");
}

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

type Hash = Uint256;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut opt: Opt = Opt::parse();
    opt.process()?;

    let mut builder = env_logger::builder();
    builder.filter_level(opt.log_level()).parse_default_env();
    if opt.altlogs {
        builder.format(|buf, record| {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f%:z");
            writeln!(buf, "{} [{:>5}] {}", timestamp, record.level(), record.args())
        });
    }
    builder.init();

    let throttle = opt.throttle.map(Duration::from_millis);

    loop {
        let mut client =
            KaspadHandler::connect(opt.kaspad_address.clone(), opt.mining_address.clone(), opt.mine_when_not_synced)
                .await?;
        if let Some(devfund_address) = &opt.devfund_address {
            client.add_devfund(devfund_address.clone(), opt.devfund_percent);
            info!(
                "devfund enabled, mining {}.{}% of the time to devfund address: {} ",
                opt.devfund_percent / 100,
                opt.devfund_percent % 100,
                devfund_address
            );
        }
        client.client_send(NotifyBlockAddedRequestMessage {}).await?;
        client.client_get_block_template().await?;
        let mut miner_manager = MinerManager::new(client.send_channel.clone(), opt.num_threads, throttle);
        client.listen(&mut miner_manager).await?;
        warn!("Disconnected from kaspad, retrying");
    }
}
