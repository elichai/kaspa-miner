use crate::proto::kaspad_message::Payload;
use crate::proto::{
    GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage,
    NotifyBlockAddedRequestMessage,
};

impl KaspadMessage {
    pub fn get_info_request() -> Self {
        KaspadMessage {
            payload: Some(Payload::GetInfoRequest(GetInfoRequestMessage {})),
        }
    }
    pub fn notify_block_added() -> Self {
        KaspadMessage {
            payload: Some(Payload::NotifyBlockAddedRequest(
                NotifyBlockAddedRequestMessage {},
            )),
        }
    }
}

impl From<GetInfoRequestMessage> for KaspadMessage {
    fn from(a: GetInfoRequestMessage) -> Self {
        KaspadMessage {
            payload: Some(Payload::GetInfoRequest(a)),
        }
    }
}
impl From<NotifyBlockAddedRequestMessage> for KaspadMessage {
    fn from(a: NotifyBlockAddedRequestMessage) -> Self {
        KaspadMessage {
            payload: Some(Payload::NotifyBlockAddedRequest(a)),
        }
    }
}

impl From<GetBlockTemplateRequestMessage> for KaspadMessage {
    fn from(a: GetBlockTemplateRequestMessage) -> Self {
        KaspadMessage {
            payload: Some(Payload::GetBlockTemplateRequest(a)),
        }
    }
}
