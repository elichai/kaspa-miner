#![cfg_attr(all(test, feature = "bench"), feature(test))]

use std::error::Error as StdError;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use chrono::Local;
use clap::Parser;
use log::{info, warn};
use std::io::Write;
use std::time::Duration;

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
mod swap_rust;
mod target;

pub mod proto {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("protowire");
}

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

type Hash = Uint256;

#[derive(Debug, Clone)]
pub struct ShutdownHandler(Arc<AtomicBool>);

pub struct ShutdownOnDrop(ShutdownHandler);

impl ShutdownHandler {
    #[inline(always)]
    pub fn is_shutdown(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn arm(&self) -> ShutdownOnDrop {
        ShutdownOnDrop(self.clone())
    }
}

impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        self.0 .0.store(true, Ordering::Release);
    }
}

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
    let shutdown = ShutdownHandler(Arc::new(AtomicBool::new(false)));
    let _shutdown_when_dropped = shutdown.arm();

    while !shutdown.is_shutdown() {
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

        let mut miner_manager =
            MinerManager::new(client.send_channel.clone(), opt.num_threads, throttle, shutdown.clone());
        client.listen(&mut miner_manager, shutdown.clone()).await?;
        warn!("Disconnected from kaspad, retrying");
    }
    Ok(())
}
