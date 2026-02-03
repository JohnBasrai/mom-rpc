use crate::{Error, Result};
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Type-erased async handler function.
///
/// Handlers take request payload bytes and return response payload bytes.
/// Routing, correlation, and transport concerns are handled by the server.
pub(super) type BoxedHandler =
    Arc<dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes>> + Send>> + Send + Sync>;

/// Wrap a typed handler function into a type-erased handler.
///
/// This allows the server to store handlers of different request/response
/// types in the same HashMap, while centralizing serialization logic.
///
/// Typed handlers operate on deserialized request values and return typed
/// response values. Serialization errors are surfaced as `Error::Serialization`.
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
            // Deserialize request payload
            let req: Req = serde_json::from_slice(&bytes).map_err(Error::from)?;

            // Execute typed handler
            let resp: Resp = handler(req).await?;

            // Serialize response payload
            let resp_bytes = serde_json::to_vec(&resp).map_err(Error::from)?;

            Ok(Bytes::from(resp_bytes))
        });

        fut as Pin<Box<dyn Future<Output = Result<Bytes>> + Send>>
    })
}
