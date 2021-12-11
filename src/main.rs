#![cfg_attr(all(test, feature = "bench"), feature(test))]

use crate::cli::Opt;
use crate::client::KaspadHandler;
use crate::proto::NotifyBlockAddedRequestMessage;
use crate::target::Uint256;
use log::warn;
use std::error::Error as StdError;
use structopt::StructOpt;

mod cli;
mod client;
mod kaspad_messages;
mod miner;
mod pow;
mod target;

pub mod proto {
    tonic::include_proto!("protowire");
    // include!("protowire.rs"); // FIXME: https://github.com/intellij-rust/intellij-rust/issues/6579
}

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

type Hash = Uint256;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut opt: Opt = Opt::from_args();
    opt.process()?;
    env_logger::builder().filter_level(opt.log_level()).parse_default_env().init();

    loop {
        let mut client =
            KaspadHandler::connect(opt.kaspad_address.clone(), opt.mining_address.clone(), opt.mine_when_not_synced)
                .await?;
        client.client_send(NotifyBlockAddedRequestMessage {}).await?;
        client.client_get_block_template().await?;

        client.listen(opt.num_threads).await?;
        warn!("Disconnected from kaspad, retrying");
    }
}
