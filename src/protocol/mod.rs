/// Protocol types for RPC message correlation and serialization
///
/// This module defines the wire format for request/response messages
/// and correlation ID management.
mod correlation;
mod message;

pub use correlation::CorrelationId;
pub use message::{RpcRequest, RpcResponse};
