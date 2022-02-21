use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

pub mod stratum;
pub mod grpc;

use crate::{Error, MinerManager};
use crate::pow::BlockSeed;

#[async_trait(?Send)]
pub trait Client {
    fn add_devfund(&mut self, address: String, percent: u16);
    async fn register(&mut self) -> Result<(), Error>;
    async fn listen(&mut self, miner: &mut MinerManager) -> Result<(), Error>;
    fn get_send_channel(&self) -> Sender<BlockSeed>;
}