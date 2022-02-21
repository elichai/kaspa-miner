use std::pin::Pin;
use futures::prelude::*;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use crate::proto::RpcBlock;

mod statum_codec;

use statum_codec::NewLineJsonCodec;
use crate::{miner::MinerManager, Error, Uint256};
use log::{info, warn};
use rand::{RngCore, thread_rng};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::{ReceiverStream};
use crate::client::Client;
use async_trait::async_trait;
use crate::client::stratum::statum_codec::{NewLineJsonCodecError, StratumLine};
use futures_util::TryStreamExt;
use num::Float;
use tokio_util::sync::PollSender;
use crate::client::stratum::statum_codec::StratumCommand;

//const DIFFICULTY_1_TARGET: Uint256 = Uint256([0x00000000ffff0000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);
const DIFFICULTY_1_TARGET: (u64, i16) = (0xffffu64, 208); // 0xffff 2^208

#[allow(dead_code)]
pub struct StratumHandler {
    //client: Framed<TcpStream, NewLineJsonCodec>,
    send_channel: Sender<StratumLine>,
    stream: Pin<Box<dyn Stream<Item = Result<StratumLine, NewLineJsonCodecError>>>>,
    miner_address: String,
    mine_when_not_synced: bool,
    devfund_address: Option<String>,
    devfund_percent: u16,
    block_template_ctr: u16,

    target_pool: Uint256,
    target_real: Uint256,
    nonce_mask: u64,
    nonce_fixed: u64
}

#[async_trait(?Send)]
impl Client for StratumHandler {
    fn add_devfund(&mut self, address: String, percent: u16) {
        self.devfund_address = Some(address);
        self.devfund_percent = percent;
    }

    async fn register(&mut self) -> Result<(), Error> {
        self.send_channel.send(StratumLine::StratumCommand(StratumCommand::Subscribe{id: 1, params: ("test1".into(), "0xffffffff".into()), error: None })).await?;
        self.send_channel.send(StratumLine::StratumCommand(StratumCommand::Authorize{id: 2, params: (self.miner_address.clone(), "x".into()), error: None })).await?;
        Ok(())
    }

    async fn listen(&mut self, miner: &mut MinerManager) -> Result<(), Error> {
        info!("Waiting for stuff");
        loop {
            match self.stream.try_next().await? {
                Some(msg) => {
                    match self.handle_message(msg, miner).await {
                        Ok(()) => {},
                        Err(e) => warn!("failed handling message: {}", e)
                    }
                },
                None => warn!("stratum message payload is empty"),
            }
        }
    }

    fn get_send_channel(&self) -> Sender<RpcBlock> {
        let (send, recv) = mpsc::channel::<RpcBlock>(1);
        let forwarding = self.send_channel.clone();
        let address = self.miner_address.clone();
        tokio::spawn(async move {
            ReceiverStream::new(recv).map(|block| StratumLine::StratumCommand(StratumCommand::MiningSubmit{
                id: 0,
                params: (address.clone(), "1e".into(), format!("{:#08x}", block.header.unwrap().nonce)), //TODO: get the id from the block somehow
                error: None
            })).map(Ok).forward(PollSender::new(forwarding)).await
        });
        send
    }
}

impl StratumHandler {
    pub async fn connect(address: String, miner_address: String, mine_when_not_synced: bool) -> Result<Box<Self>, Error>
    {
        info!("Connecting to {}", address);
        let socket = TcpStream::connect(address).await.unwrap();

        let client = Framed::new(socket, NewLineJsonCodec::new());
        let (send_channel, recv) = mpsc::channel::<StratumLine>(3);
        let (sink, stream) = client.split();
        tokio::spawn(async move {
            ReceiverStream::new(recv).map(Ok).forward(sink).await
        });

        Ok(Box::new(Self {
            //client,
            stream: Box::pin(stream),
            send_channel,
            miner_address,
            mine_when_not_synced,
            devfund_address: None,
            devfund_percent: 0,
            block_template_ctr: (thread_rng().next_u64() % 10_000u64) as u16,
            target_pool: Default::default(),
            target_real: Default::default(),
            nonce_mask: 0,
            nonce_fixed: 0
        }))
    }

    async fn handle_message(&mut self, msg: StratumLine, miner: &mut MinerManager) -> Result<(), Error> {
        match msg {
            StratumLine::StratumResult { .. } => {
                warn!("Ignoring result for now");
                Ok(())
            }
            StratumLine::StratumCommand(StratumCommand::SetExtranonce { params: (ref extranonce, ref nonce_size), ref error, .. }) if error.is_none() => {
                self.nonce_fixed = u64::from_str_radix(extranonce.as_str(), 16)? << (nonce_size*8);
                self.nonce_mask = (1 << (nonce_size*8))-1;
                Ok(())
            },
            StratumLine::StratumCommand(StratumCommand::MiningSetDifficulty { params: (ref difficulty,), ref error, .. }) if error.is_none() => {
                let mut buf = [0u64, 0u64, 0u64, 0u64];
                let (mantissa, exponent, _) = difficulty.recip().integer_decode();
                let new_mantissa = mantissa*DIFFICULTY_1_TARGET.0;
                let new_exponent = (DIFFICULTY_1_TARGET.1 + exponent) as u64;
                let start = (new_exponent / 64) as usize;
                let remainder = new_exponent % 64;

                buf[start] = new_mantissa << remainder;        // bottom
                if start < 3 {
                    buf[start + 1] = new_mantissa >> 64 - remainder; // top
                } else if new_mantissa.leading_zeros() < remainder as u32 {
                    return Err("Target is too big".into());
                }


                self.target_pool = Uint256::new(buf);
                info!("Difficulty: {:?}, Target: {:?}", difficulty, self.target_pool);
                Ok(())
            },
            StratumLine::StratumCommand(StratumCommand::MiningNotify { ref params, ref error, .. }) if error.is_none() => {
                info!("{:?}", msg);
                Ok(())
            },
            _ => Err(format!("Unhandled stratum response: {:?}", msg).into()),
        }
    }
}