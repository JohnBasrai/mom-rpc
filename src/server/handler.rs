use crate::error::Result;
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Type-erased async handler function
///
/// Handlers take request bytes and return response bytes.
/// The actual request/response types are handled by the wrapper created
/// in `RpcServer::handle()`.
///
/// Wrapped in Arc for cheap cloning when spawning tasks.
pub(super) type BoxedHandler =
    Arc<dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes>> + Send>> + Send + Sync>;

/// Wrap a typed handler function into a type-erased handler
///
/// This allows the server to store handlers of different types in the same HashMap.
pub(super) fn wrap_handler<F, Fut, Req, Resp>(handler: F) -> BoxedHandler
where
    F: Fn(Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Resp>> + Send + 'static,
    Req: DeserializeOwned + Send + 'static,
    Resp: Serialize + Send + 'static,
{
    // ---
    Arc::new(move |bytes: Bytes| {
        // Deserialize request
        let request_result: Result<Req> = serde_json::from_slice(&bytes).map_err(Into::into);

        let fut = match request_result {
            Ok(request) => {
                // Call handler
                let handler_future = handler(request);

                // Box the future
                Box::pin(async move {
                    let response = handler_future.await?;
                    let response_bytes = serde_json::to_vec(&response)?;
                    Ok(Bytes::from(response_bytes))
                }) as Pin<Box<dyn Future<Output = Result<Bytes>> + Send>>
            }
            Err(e) => {
                // Return error future
                Box::pin(async move { Err(e) })
                    as Pin<Box<dyn Future<Output = Result<Bytes>> + Send>>
            }
        };

        fut
    })
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize)]
    struct TestRequest {
        // ---
        value: i32,
    }

    #[derive(Serialize)]
    struct TestResponse {
        // ---
        result: i32,
    }

    #[tokio::test]
    async fn test_wrap_handler() {
        // ---
        let handler = |req: TestRequest| async move {
            Ok(TestResponse {
                result: req.value * 2,
            })
        };

        let wrapped = wrap_handler(handler);

        let request = TestRequest { value: 21 };
        let request_bytes = serde_json::to_vec(&request).unwrap();

        let response_bytes = wrapped(Bytes::from(request_bytes)).await.unwrap();
        let response: TestResponse = serde_json::from_slice(&response_bytes).unwrap();

        assert_eq!(response.result, 42);
    }
}
