use crate::client::Client;
use crate::pow::BlockSeed;
use crate::pow::BlockSeed::{FullBlock, PartialBlock};
use crate::proto::kaspad_message::Payload;
use crate::proto::rpc_client::RpcClient;
use crate::proto::{
    GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage, NotifyBlockAddedRequestMessage,
};
use crate::{miner::MinerManager, Error};
use async_trait::async_trait;
use futures_util::StreamExt;
use log::{error, info, warn};
use rand::{thread_rng, RngCore};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::SendError, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::PollSender;
use tonic::{transport::Channel as TonicChannel, Streaming};

#[allow(dead_code)]
pub struct KaspadHandler {
    client: RpcClient<TonicChannel>,
    pub send_channel: Sender<KaspadMessage>,
    stream: Streaming<KaspadMessage>,
    miner_address: String,
    mine_when_not_synced: bool,
    devfund_address: Option<String>,
    devfund_percent: u16,
    block_template_ctr: Arc<AtomicU16>,
}

#[async_trait(?Send)]
impl Client for KaspadHandler {
    fn add_devfund(&mut self, address: String, percent: u16) {
        self.devfund_address = Some(address);
        self.devfund_percent = percent;
    }

    async fn register(&mut self) -> Result<(), Error> {
        self.client_send(NotifyBlockAddedRequestMessage {}).await?;
        self.client_get_block_template().await?;
        Ok(())
    }

    async fn listen(&mut self, miner: &mut MinerManager) -> Result<(), Error> {
        while let Some(msg) = self.stream.message().await? {
            match msg.payload {
                Some(payload) => self.handle_message(payload, miner).await?,
                None => warn!("kaspad message payload is empty"),
            }
        }
        Ok(())
    }

    fn get_send_channel(&self) -> Sender<BlockSeed> {
        // KaspadMessage::submit_block(block)
        let (send, recv) = mpsc::channel::<BlockSeed>(1);
        let forwarding = self.send_channel.clone();
        tokio::spawn(async move {
            ReceiverStream::new(recv)
                .map(|block_seed| match block_seed {
                    FullBlock(block) => KaspadMessage::submit_block(block),
                    PartialBlock { .. } => unreachable!("All blocks sent here should have arrived from here"),
                })
                .map(Ok)
                .forward(PollSender::new(forwarding))
                .await
        });
        send
    }
}

impl KaspadHandler {
    pub async fn connect<D>(
        address: D,
        miner_address: String,
        mine_when_not_synced: bool,
        block_template_ctr: Option<Arc<AtomicU16>>,
    ) -> Result<Box<Self>, Error>
    where
        D: std::convert::TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Error>,
    {
        let mut client = RpcClient::connect(address).await?;
        let (send_channel, recv) = mpsc::channel(3);
        send_channel.send(GetInfoRequestMessage {}.into()).await?;
        send_channel.send(GetBlockTemplateRequestMessage { pay_address: miner_address.clone() }.into()).await?;
        let stream = client.message_stream(ReceiverStream::new(recv)).await?.into_inner();
        Ok(Box::new(Self {
            client,
            stream,
            send_channel,
            miner_address,
            mine_when_not_synced,
            devfund_address: None,
            devfund_percent: 0,
            block_template_ctr: block_template_ctr
                .unwrap_or(Arc::new(AtomicU16::new((thread_rng().next_u64() % 10_000u64) as u16))),
        }))
    }

    async fn client_send(&self, msg: impl Into<KaspadMessage>) -> Result<(), SendError<KaspadMessage>> {
        self.send_channel.send(msg.into()).await
    }

    async fn client_get_block_template(&mut self) -> Result<(), SendError<KaspadMessage>> {
        let pay_address = match &self.devfund_address {
            Some(devfund_address) if self.block_template_ctr.load(Ordering::SeqCst) <= self.devfund_percent => {
                devfund_address.clone()
            }
            _ => self.miner_address.clone(),
        };
        self.block_template_ctr.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some((v + 1) % 10_000)).unwrap();
        self.client_send(GetBlockTemplateRequestMessage { pay_address }).await
    }

    async fn handle_message(&mut self, msg: Payload, miner: &mut MinerManager) -> Result<(), Error> {
        match msg {
            Payload::BlockAddedNotification(_) => self.client_get_block_template().await?,
            Payload::GetBlockTemplateResponse(template) => match (template.block, template.is_synced, template.error) {
                (Some(b), true, None) => miner.process_block(Some(FullBlock(b))).await?,
                (Some(b), false, None) if self.mine_when_not_synced => miner.process_block(Some(FullBlock(b))).await?,
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
