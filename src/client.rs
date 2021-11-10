use crate::proto::kaspad_message::Payload;
use crate::proto::rpc_client::RpcClient;
use crate::proto::{
    GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage, RpcBlock,
};
use crate::Error;
use sha3::CShake256;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Sender;
use tokio::task;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel as TonicChannel;
use tonic::Streaming;
pub struct GrpcClient {
    client: RpcClient<TonicChannel>,
    listen_handler: JoinHandle<Result<KaspadHandler, Error>>,
    send_channel: Sender<KaspadMessage>,
}

pub struct KaspadHandler {
    send_channel: Sender<KaspadMessage>,
    stream: Streaming<KaspadMessage>,
    miner_address: String,
}

struct MiningBlock {
    block: RpcBlock,
    start_state: CShake256,
}

fn process_block(block: &RpcBlock) {}

impl KaspadHandler {
    pub fn listen(
        stream: Streaming<KaspadMessage>,
        send_channel: Sender<KaspadMessage>,
        miner_address: String,
    ) -> JoinHandle<Result<KaspadHandler, Error>> {
        task::spawn(async move {
            let mut handler = Self {
                send_channel,
                stream,
                miner_address,
            };
            while let Some(msg) = handler.stream.message().await? {
                match &msg.payload {
                    Some(payload) => match payload {
                        Payload::BlockAddedNotification(_) => {
                            let get_block_template = GetBlockTemplateRequestMessage {
                                pay_address: handler.miner_address.clone(),
                            };
                            handler.send_channel.send(get_block_template.into()).await?;
                        }
                        Payload::GetBlockTemplateResponse(template) => {
                            if let Some(err) = &template.error {
                                println!("Error! {:?}", err);
                                continue;
                            }
                            let block = match &template.block {
                                Some(b) => process_block(b),
                                None => {
                                    println!("No block and No Error!");
                                    continue;
                                }
                            };
                        }
                        _ => println!("got unknown msg: {:?}", msg),
                    },
                    None => println!("payload is empty"),
                }
            }
            Ok(handler)
        })
    }

    // async fn handle_msg(&self, msg: Payload) -> Result<(), Error> {
    //     match msg {
    //         Payload::BlockAddedNotification(notf) => {
    //             let get_block_template = GetBlockTemplateRequestMessage{ pay_address: self.miner_address.clone() };
    //             let send = self.client_send(get_block_template);
    //             let send = send.await;
    //             send?;
    //         }
    //         _ => println!("got unknown msg: {:?}", msg),
    //     }
    //     Ok(())
    // }
}

impl GrpcClient {
    pub async fn connect<D>(address: D, miner_address: String) -> Result<Self, Error>
    where
        D: std::convert::TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Error>,
    {
        let mut client = RpcClient::connect(address).await?;
        let (send_channel, recv) = mpsc::channel(32);
        send_channel.send(GetInfoRequestMessage {}.into()).await?;
        let stream = client
            .message_stream(ReceiverStream::new(recv))
            .await?
            .into_inner();
        let listen_handler = KaspadHandler::listen(stream, send_channel.clone(), miner_address);
        Ok(Self {
            client,
            listen_handler,
            send_channel,
        })
    }

    pub async fn client_send(
        &self,
        msg: impl Into<KaspadMessage>,
    ) -> Result<(), SendError<KaspadMessage>> {
        self.send_channel.send(msg.into()).await
    }
}
