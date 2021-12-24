use crate::proto::kaspad_message::Payload;
use crate::proto::rpc_client::RpcClient;
use crate::proto::{GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage};
use crate::{miner::MinerManager, Error};
use log::{error, info, warn};
use tokio::sync::mpsc::{self, error::SendError, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Channel as TonicChannel, Streaming};

#[allow(dead_code)]
pub struct KaspadHandler {
    client: RpcClient<TonicChannel>,
    send_channel: Sender<KaspadMessage>,
    stream: Streaming<KaspadMessage>,
    miner_address: String,
    mine_when_not_synced: bool,
}

impl KaspadHandler {
    pub async fn connect<D>(address: D, miner_address: String, mine_when_not_synced: bool) -> Result<Self, Error>
    where
        D: std::convert::TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Error>,
    {
        let mut client = RpcClient::connect(address).await?;
        let (send_channel, recv) = mpsc::channel(3);
        send_channel.send(GetInfoRequestMessage {}.into()).await?;
        send_channel.send(GetBlockTemplateRequestMessage { pay_address: miner_address.clone() }.into()).await?;
        let stream = client.message_stream(ReceiverStream::new(recv)).await?.into_inner();
        Ok(Self { client, stream, send_channel, miner_address, mine_when_not_synced })
    }

    pub async fn client_send(&self, msg: impl Into<KaspadMessage>) -> Result<(), SendError<KaspadMessage>> {
        self.send_channel.send(msg.into()).await
    }

    pub async fn client_get_block_template(&self) -> Result<(), SendError<KaspadMessage>> {
        self.client_send(GetBlockTemplateRequestMessage { pay_address: self.miner_address.clone() }).await
    }

    pub async fn listen(
        &mut self,
        num_threads: usize,
        cuda_device: Vec<u16>,
        workload: Vec<f32>,
        workload_absolute: bool,
    ) -> Result<(), Error> {
        let mut miner =
            MinerManager::new(self.send_channel.clone(), num_threads, cuda_device, workload, workload_absolute);
        while let Some(msg) = self.stream.message().await? {
            match msg.payload {
                Some(payload) => self.handle_message(payload, &mut miner).await?,
                None => warn!("kaspad message payload is empty"),
            }
        }
        Ok(())
    }

    async fn handle_message(&self, msg: Payload, miner: &mut MinerManager) -> Result<(), Error> {
        match msg {
            Payload::BlockAddedNotification(_) => self.client_get_block_template().await?,
            Payload::GetBlockTemplateResponse(template) => match (template.block, template.is_synced, template.error) {
                (Some(b), true, None) => miner.process_block(Some(b)).await?,
                (Some(b), false, None) if self.mine_when_not_synced => miner.process_block(Some(b)).await?,
                (_, false, None) => miner.process_block(None).await?,
                (_, _, Some(e)) => warn!("GetTemplate returned with an error: {:?}", e),
                (None, true, None) => error!("No block and No Error!"),
            },
            Payload::SubmitBlockResponse(res) => match res.error {
                None => info!("block submitted successfully!"),
                Some(e) => warn!("Failed submitting block: {:?}", e),
            },
            Payload::GetBlockResponse(msg) => {
                if let Some(e) = msg.error {
                    return Err(e.message.into());
                } else {
                    info!("Get block response: {:?}", msg);
                }
            }
            Payload::GetInfoResponse(info) => info!("Kaspad version: {}", info.server_version),
            Payload::NotifyBlockAddedResponse(res) => match res.error {
                None => info!("Registered for block notifications"),
                Some(e) => error!("Failed registering for block notifications: {:?}", e),
            },
            msg => info!("got unknown msg: {:?}", msg),
        }
        Ok(())
    }
}
