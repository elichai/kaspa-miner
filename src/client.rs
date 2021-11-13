use crate::miner::MinerManager;
use crate::proto::kaspad_message::Payload;
use crate::proto::rpc_client::RpcClient;
use crate::proto::{GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage};
use crate::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel as TonicChannel;
use tonic::Streaming;

pub struct KaspadHandler {
    client: RpcClient<TonicChannel>,
    send_channel: Sender<KaspadMessage>,
    stream: Streaming<KaspadMessage>,
    miner_address: String,
}

impl KaspadHandler {
    pub async fn connect<D>(address: D, miner_address: String) -> Result<Self, Error>
    where
        D: std::convert::TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Error>,
    {
        let mut client = RpcClient::connect(address).await?;
        let (send_channel, recv) = mpsc::channel(1);
        send_channel.send(GetInfoRequestMessage {}.into()).await?;
        let stream = client
            .message_stream(ReceiverStream::new(recv))
            .await?
            .into_inner();
        Ok(Self {
            client,
            stream,
            send_channel,
            miner_address,
        })
    }

    pub async fn client_send(
        &self,
        msg: impl Into<KaspadMessage>,
    ) -> Result<(), SendError<KaspadMessage>> {
        self.send_channel.send(msg.into()).await
    }

    pub async fn listen(&mut self) -> Result<(), Error> {
        let mut miner = MinerManager::new(self.send_channel.clone());
        while let Some(msg) = self.stream.message().await? {
            match msg.payload {
                Some(payload) => self.handle_message(payload, &mut miner).await?,
                None => println!("payload is empty"),
            }
        }
        Ok(())
    }

    async fn handle_message(&self, msg: Payload, miner: &mut MinerManager) -> Result<(), Error> {
        match msg {
            Payload::BlockAddedNotification(_) => {
                let get_block_template = GetBlockTemplateRequestMessage {
                    pay_address: self.miner_address.clone(),
                };
                self.send_channel.send(get_block_template.into()).await?;
            }
            Payload::GetBlockTemplateResponse(template) => match (template.block, template.error) {
                (Some(b), None) => {
                    if let Err(e) = miner.process_block(b).await {
                        println!("Failed processing block: {}", e);
                    }
                }
                (None, Some(e)) => println!("GetTemplate returned with an error: {:?}", e),
                (Some(_), Some(e)) => println!("GetTemplate returned with block&error: {:?}", e),
                (None, None) => println!("No block and No Error!"),
            },
            Payload::SubmitBlockResponse(res) => match res.error {
                None => println!("Submitted block successfully!"),
                Some(e) => println!("Failed submitting block: {:?}", e),
            },
            msg => println!("got unknown msg: {:?}", msg),
        }
        Ok(())
    }
}
