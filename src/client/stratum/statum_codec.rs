use std::{fmt, io};
use bytes::BytesMut;
use log::debug;
use serde::{Deserialize, Serialize};
use tokio_util::codec::{LinesCodec, Decoder, Encoder};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag="method")]
pub(crate) enum StratumCommand {
    #[serde(rename = "set_extranonce")]
    SetExtranonce{id: u32, params: (String, u32), error:Option<String>},
    #[serde(rename = "mining.set_difficulty")]
    MiningSetDifficulty{id: u32, params: (f32,), error:Option<String>},
    #[serde(rename = "mining.notify")]
    MiningNotify{id: u32, params: (String, Vec<u64>, u64), error:Option<String>},
    #[serde(rename = "subscribe")]
    Subscribe{id:u32, params: (String, String), error: Option<String>},
    #[serde(rename = "authorize")]
    Authorize{id:u32, params: (String, String), error: Option<String>},
    #[serde(rename = "mining.submit")]
    MiningSubmit{id: u32, params: (String, String, String), error: Option<String>}
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum StratumLine {
    StratumCommand(StratumCommand),
    StratumResult {
        id: u32,
        result: bool,
        error: Option<String>
    }
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
        Self{ lines_codec: LinesCodec::new() }
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
            Err(_) => { Err(NewLineJsonCodecError::LineSplitError) }
            _ => { Ok(None) }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        debug!("Finalizing decoding");
        match self.lines_codec.decode_eof(buf) {
            Ok(Some(s)) => {
                serde_json::from_str(s.as_str()).map_err(|e| (e.to_string(), s).into())
            }
            Err(_) => { Err(NewLineJsonCodecError::LineSplitError) }
            _ => { Ok(None) }
        }
    }
}

impl Encoder<StratumLine> for NewLineJsonCodec {
    type Error = NewLineJsonCodecError;

    fn encode(&mut self, item: StratumLine, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if let Ok( json) = serde_json::to_string(&item) {
            return self.lines_codec.encode(json, dst).map_err(|_| NewLineJsonCodecError::LineEncodeError);
        }
        return Err(NewLineJsonCodecError::JsonEncodeError);
    }
}

impl Default for NewLineJsonCodec {
    fn default() -> Self {
        Self::new()
    }
}