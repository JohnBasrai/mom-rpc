//! Handler type erasure and wrapping.
//!
//! This module provides the [`BoxedHandler`] type and [`wrap_handler`] function
//! which enable the server to store handlers with different request/response
//! types in a single `HashMap<String, BoxedHandler>`.
//!
//! # Type Erasure Strategy
//!
//! User handlers are strongly typed functions:
//! ```ignore
//! async fn add(req: AddRequest) -> Result<AddResponse>
//! ```
//!
//! These are wrapped into a common signature that operates on raw bytes:
//! ```ignore
//! Fn(Bytes) -> Future<Output = Result<Bytes>>
//! ```
//!
//! The wrapper handles serialization/deserialization automatically, surfacing
//! errors as [`RpcError::Serialization`] when payloads don't match the expected
//! schema.

use crate::{Result, RpcError};
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
/// response values. Serialization errors are surfaced as `RpcError::Serialization`.
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
            let req: Req = serde_json::from_slice(&bytes).map_err(RpcError::from)?;

            // Execute typed handler
            let resp: Resp = handler(req).await?;

            // Serialize response payload
            let resp_bytes = serde_json::to_vec(&resp).map_err(RpcError::from)?;

            Ok(Bytes::from(resp_bytes))
        });

        fut as Pin<Box<dyn Future<Output = Result<Bytes>> + Send>>
    })
}
