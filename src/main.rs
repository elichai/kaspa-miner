use crate::cli::Opt;
use crate::client::KaspadHandler;
use crate::proto::NotifyBlockAddedRequestMessage;
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

pub type Hash = [u8; 32];
pub type Error = Box<dyn StdError + Send + Sync + 'static>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut opt: Opt = Opt::from_args();
    opt.process()?;
    env_logger::builder()
        .filter_level(opt.log_level())
        .parse_default_env()
        .init();

    let mut client = KaspadHandler::connect(opt.kaspad_address, opt.mining_address).await?;

    client
        .client_send(NotifyBlockAddedRequestMessage {})
        .await?;

    client.listen(opt.num_threads).await
}
