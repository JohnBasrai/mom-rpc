use crate::error::{Error, Result};
use crate::protocol::{RpcRequest, RpcResponse};
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub(super) type HandlerOutput = (String, Bytes);

/// Type-erased async handler function
///
/// Handlers take request bytes (full RpcRequest envelope) and return a tuple:
/// (response_topic, response_bytes).
///
/// Wrapped in Arc for cheap cloning when spawning tasks.
pub(super) type BoxedHandler =
    Arc<dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<HandlerOutput>> + Send>> + Send + Sync>;

/// Wrap a typed handler function into a type-erased handler
///
/// This allows the server to store handlers of different types in the same HashMap.
pub(super) fn wrap_handler<F, Fut, Req, Resp>(handler: F) -> BoxedHandler
where
    F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<Resp>> + Send + 'static,
    Req: DeserializeOwned + Send + 'static,
    Resp: Serialize + Send + 'static,
{
    // ---
    Arc::new(move |bytes: Bytes| {
        let handler = handler.clone();
        let fut = Box::pin(async move {
            // ---
            // Deserialize full request envelope
            let request: RpcRequest<Req> = serde_json::from_slice(&bytes).map_err(Error::from)?;

            // Execute typed handler on payload
            let resp_payload: Resp = handler(request.payload).await?;

            // Serialize response envelope
            let response = RpcResponse {
                correlation_id: request.correlation_id,
                payload: resp_payload,
            };
            let response_bytes = serde_json::to_vec(&response)?;

            Ok((request.response_topic, Bytes::from(response_bytes)))
        });

        fut as Pin<Box<dyn Future<Output = Result<HandlerOutput>> + Send>>
    })
}
