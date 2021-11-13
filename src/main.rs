mod client;
mod kaspad_messages;
mod miner;
mod pow;
mod target;

pub mod proto {
    // tonic::include_proto!("protowire"); // FIXME: https://github.com/intellij-rust/intellij-rust/issues/6579
    include!("protowire.rs");
}
use crate::client::KaspadHandler;
use crate::proto::NotifyBlockAddedRequestMessage;
use std::error::Error as StdError;

pub type Hash = [u8; 32];

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let miner_address =
        "kaspa:qq9frgnvaa3zg9qhh9t8s6vkf6x7rxqlnemgcj72y9enf2hsujxd276dtukjm".to_string();
    let mut client = KaspadHandler::connect("grpc://localhost:16110", miner_address).await?;

    client
        .client_send(NotifyBlockAddedRequestMessage {})
        .await?;

    client.listen().await
}
