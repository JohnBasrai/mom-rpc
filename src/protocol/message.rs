use crate::protocol::CorrelationId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcRequest<TReq> {
    pub correlation_id: CorrelationId,
    pub response_topic: String,
    pub payload: TReq,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcResponse<TResp> {
    pub correlation_id: CorrelationId,
    pub payload: TResp,
}
