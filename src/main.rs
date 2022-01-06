#![cfg_attr(all(test, feature = "bench"), feature(test))]

use std::error::Error as StdError;

use log::{info, warn};
use clap::{App,IntoApp,Parser,Args};
use work_manager::GPUWorkFactory;

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
    tonic::include_proto!("protowire");
    // include!("protowire.rs"); // FIXME: https://github.com/intellij-rust/intellij-rust/issues/6579
}

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

type Hash = Uint256;

#[tokio::main]
async fn main() -> Result<(), Error> {
    //let app: App = work_manager::Opt2::augment_args_for_update(Opt::into_app()); //.subcommands(work_manager::plugin_subcommands());
    let (app, factory): (App, GPUWorkFactory) = work_manager::load_plugins(Opt::into_app(), &["plugins/cuda/target/debug/libkaspacuda.so"])?;

    let matches = app.get_matches();
    let mut opt: Opt = Opt::from_clap(&matches);
    opt.process()?;
    env_logger::builder().filter_level(opt.log_level()).parse_default_env().init();

    loop {
        let mut client =
            KaspadHandler::connect(opt.kaspad_address.clone(), opt.mining_address.clone(), opt.mine_when_not_synced)
                .await?;
        if opt.devfund_percent > 0 {
            client.add_devfund( opt.devfund_address.clone(), opt.devfund_percent);
            info!(
                "devfund enabled, mining {}.{}% of the time to devfund address: {} ",
                opt.devfund_percent / 100,
                opt.devfund_percent % 100,
                opt.devfund_address
            );
        }
        client.client_send(NotifyBlockAddedRequestMessage {}).await?;
        client.client_get_block_template().await?;
        let mut miner_manager = MinerManager::new(client.send_channel.clone(), opt.num_threads, &factory);
        client.listen(&mut miner_manager).await?;
        warn!("Disconnected from kaspad, retrying");
    }
}
