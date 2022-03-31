use bytes::BytesMut;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::*;
use std::fmt::{Display, Formatter};
use std::{fmt, io};
use tokio_util::codec::{Decoder, Encoder, LinesCodec};

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone)]
#[repr(u8)]
pub enum ErrorCode {
    Unknown = 20,
    JobNotFound = 21,
    DuplicateShare = 22,
    LowDifficultyShare = 23,
    Unauthorized = 24,
    NotSubscribed = 25,
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            ErrorCode::Unknown => write!(f, "Unknown"),
            ErrorCode::JobNotFound => write!(f, "JobNotFound"),
            ErrorCode::DuplicateShare => write!(f, "DuplicateShare"),
            ErrorCode::LowDifficultyShare => write!(f, "LowDifficultyShare"),
            ErrorCode::Unauthorized => write!(f, "Unauthorized"),
            ErrorCode::NotSubscribed => write!(f, "NotSubscribed"),
        }
    }
}

type StratumError = Option<(ErrorCode, String, Option<Value>)>;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum MiningNotify {
    MiningNotifyShort {
        id: Option<u32>,
        params: (String, [u64; 4], u64),
        error: StratumError,
    },
    MiningNotifyLong {
        id: Option<u32>,
        params: (String, String, String, String, Vec<String>, String, String, String, bool),
        error: StratumError,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum MiningSubmit {
    MiningSubmitShort { id: u32, params: (String, String, String), error: StratumError },
    MiningSubmitLong { id: u32, params: (String, String, String, String, String), error: StratumError },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "method")]
pub(crate) enum StratumCommand {
    #[serde(rename = "set_extranonce")]
    SetExtranonce { id: u32, params: (String, u32), error: StratumError },
    #[serde(rename = "mining.set_difficulty")]
    MiningSetDifficulty { id: Option<u32>, params: (f32,), error: StratumError },
    #[serde(rename = "mining.notify")]
    MiningNotify(MiningNotify),
    #[serde(rename = "mining.subscribe")]
    Subscribe { id: u32, params: (String,), error: StratumError },
    #[serde(rename = "mining.authorize")]
    Authorize { id: u32, params: (String, String), error: StratumError },
    #[serde(rename = "mining.submit")]
    MiningSubmit(MiningSubmit),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum StratumLine {
    StratumCommand(StratumCommand),
    StratumResult { id: u32, result: Option<bool>, error: StratumError },
    SubscribeResult { id: u32, result: (Vec<(String, String)>, String, u32), error: StratumError },
}

/// An error occurred while encoding or decoding a line.
#[derive(Debug)]
pub(crate) enum NewLineJsonCodecError {
    JsonParseError(String),
    JsonEncodeError,
    LineSplitError,
    LineEncodeError,
    Io(io::Error),
}

impl fmt::Display for NewLineJsonCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Some error occured")
    }
}
impl From<io::Error> for NewLineJsonCodecError {
    fn from(e: io::Error) -> NewLineJsonCodecError {
        NewLineJsonCodecError::Io(e)
    }
}
impl std::error::Error for NewLineJsonCodecError {}

impl From<(String, String)> for NewLineJsonCodecError {
    fn from(e: (String, String)) -> Self {
        NewLineJsonCodecError::JsonParseError(format!("{}: {}", e.0, e.1))
    }
}

pub(crate) struct NewLineJsonCodec {
    lines_codec: LinesCodec,
}

impl NewLineJsonCodec {
    pub fn new() -> Self {
        Self { lines_codec: LinesCodec::new() }
    }
}

impl Decoder for NewLineJsonCodec {
    type Item = StratumLine;
    type Error = NewLineJsonCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        debug!("decoding {:?}", src);
        match self.lines_codec.decode(src) {
            Ok(Some(s)) => {
                serde_json::from_str::<StratumLine>(s.as_str()).map_err(|e| (e.to_string(), s).into()).map(Some)
            }
            Err(_) => Err(NewLineJsonCodecError::LineSplitError),
            _ => Ok(None),
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        debug!("Finalizing decoding");
        match self.lines_codec.decode_eof(buf) {
            Ok(Some(s)) => serde_json::from_str(s.as_str()).map_err(|e| (e.to_string(), s).into()),
            Err(_) => Err(NewLineJsonCodecError::LineSplitError),
            _ => Ok(None),
        }
    }
}

impl Encoder<StratumLine> for NewLineJsonCodec {
    type Error = NewLineJsonCodecError;

    fn encode(&mut self, item: StratumLine, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if let Ok(json) = serde_json::to_string(&item) {
            return self.lines_codec.encode(json, dst).map_err(|_| NewLineJsonCodecError::LineEncodeError);
        }
        Err(NewLineJsonCodecError::JsonEncodeError)
    }
}

impl Default for NewLineJsonCodec {
    fn default() -> Self {
        Self::new()
    }
}
