use crate::proto::{
    kaspad_message::Payload, GetBlockTemplateRequestMessage, GetInfoRequestMessage, KaspadMessage,
    NotifyBlockAddedRequestMessage, RpcBlock, SubmitBlockRequestMessage,
};

impl KaspadMessage {
    #[inline(always)]
    pub fn get_info_request() -> Self {
        KaspadMessage { payload: Some(Payload::GetInfoRequest(GetInfoRequestMessage {})) }
    }
    #[inline(always)]
    pub fn notify_block_added() -> Self {
        KaspadMessage { payload: Some(Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage {})) }
    }

    #[inline(always)]
    pub fn submit_block(block: RpcBlock) -> Self {
        KaspadMessage { payload: Some(Payload::SubmitBlockRequest(SubmitBlockRequestMessage { block: Some(block) })) }
    }
}

impl From<GetInfoRequestMessage> for KaspadMessage {
    fn from(a: GetInfoRequestMessage) -> Self {
        KaspadMessage { payload: Some(Payload::GetInfoRequest(a)) }
    }
}
impl From<NotifyBlockAddedRequestMessage> for KaspadMessage {
    fn from(a: NotifyBlockAddedRequestMessage) -> Self {
        KaspadMessage { payload: Some(Payload::NotifyBlockAddedRequest(a)) }
    }
}

impl From<GetBlockTemplateRequestMessage> for KaspadMessage {
    fn from(a: GetBlockTemplateRequestMessage) -> Self {
        KaspadMessage { payload: Some(Payload::GetBlockTemplateRequest(a)) }
    }
}
