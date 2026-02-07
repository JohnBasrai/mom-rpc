use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;

use mom_rpc::{
    //
    create_memory_transport,
    Result,
    RpcClient,
    RpcConfig,
    RpcError,
    RpcServer,
    TransportPtr,
};

macro_rules! log_info {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::info!($($arg)*);
    };
}

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse {
    sum: i32,
}

struct MathServer {
    // ---
    handle: JoinHandle<Result<()>>,
    _server: RpcServer,
    transport: TransportPtr,
    node_id: String,
}

impl MathServer {
    // ---
    async fn new(id: &str) -> Result<Self> {
        // ---

        let config = RpcConfig::memory(id);
        let transport = create_memory_transport(&config).await?;
        // Use test-specific node_id to avoid subscription conflicts
        let node_id = format!("math-{id}");
        let server = RpcServer::with_transport(transport.clone(), node_id.clone());

        server.register("add", |req: AddRequest| async move {
            // ---
            Ok(AddResponse { sum: req.a + req.b })
        });
        let handle = server.spawn();

        // Give the server task time to subscribe before returning
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        Ok(Self {
            handle,
            _server: server,
            transport,
            node_id,
        })
    }

    async fn shutdown(self) -> Result<()> {
        // --
        // Shutdown server first (signals run loop to exit)
        self._server.shutdown().await?;

        // Then close transport
        self.transport.close().await?;

        // Wait for server task to complete
        self.handle.await.expect("server task panicked")?;

        Ok(())
    }

    fn transport(&self) -> TransportPtr {
        self.transport.clone()
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    log_info!("Starting basic request test");

    let server = MathServer::new("test_basic_request").await?;
    log_info!("after MathServer::new");

    let client = RpcClient::with_transport(server.transport(), "Sally").await?;
    log_info!("after RpcClient::new");

    log_info!("sending math add 2 3...");
    let resp: AddResponse = client
        .request_to(server.node_id(), "add", AddRequest { a: 2, b: 3 })
        .await?;
    log_info!("sending math add 2 3...done");

    assert_eq!(resp.sum, 5);

    log_info!("calling server shutdown");

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    let server = MathServer::new("test_concurrent_requests").await.unwrap();
    let client = RpcClient::with_transport(server.transport(), "George")
        .await
        .unwrap();

    let node_id = server.node_id().to_string();
    let mut handles = Vec::new();

    for i in 0..10 {
        // ---
        let c = client.clone();
        let node_id = node_id.clone();

        handles.push(tokio::spawn(async move {
            let resp: AddResponse = c
                .request_to(&node_id, "add", AddRequest { a: i, b: i })
                .await
                .unwrap();
            resp.sum
        }));
    }

    for (i, task) in handles.into_iter().enumerate() {
        let sum = task.await.unwrap();
        assert_eq!(sum, (i as i32) * 2);
    }
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_timeout() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    let config = RpcConfig::memory("test_timeout");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "lazy-math");

    server.register("add", |req: AddRequest| async move {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();

    // Give the server task time to subscribe
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = RpcClient::with_transport(transport.clone(), "Denis").await?;

    log_info!("test_timeout: sending 1 + 1");

    let fut =
        client.request_to::<AddRequest, AddResponse>("lazy-math", "add", AddRequest { a: 1, b: 1 });

    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;

    assert!(res.is_err(), "request unexpectedly completed");

    log_info!("test_timeout: {:?}", res);

    log_info!("test_timeout: shutting down server");
    server.shutdown().await?;

    log_info!("test_timeout: closing transport");
    transport.close().await?;

    log_info!("test_timeout: join handler");
    handle.await.expect("test_timeout:: server task panicked")?;

    Ok(())
}

#[tokio::test]
#[ignore] // TODO: Implement error response protocol
async fn test_error_response() -> Result<()> {
    #[cfg(feature = "logging")]
    init_logging();

    let config = RpcConfig::memory("test_error");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "error-math");

    server.register("divide", |req: AddRequest| async move {
        if req.b == 0 {
            return Err(RpcError::InvalidRequest);
        }
        Ok(AddResponse { sum: req.a / req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = RpcClient::with_transport(transport.clone(), "error-client").await?;

    // Test error case
    let result: Result<AddResponse> = client
        .request_to("error-math", "divide", AddRequest { a: 10, b: 0 })
        .await;
    assert!(result.is_err(), "expected error for division by zero");

    // Test success case
    let resp: AddResponse = client
        .request_to("error-math", "divide", AddRequest { a: 10, b: 2 })
        .await?;
    assert_eq!(resp.sum, 5);

    server.shutdown().await?;
    transport.close().await?;
    handle.await.expect("server task panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_multiple_clients() -> Result<()> {
    #[cfg(feature = "logging")]
    init_logging();

    let config = RpcConfig::memory("test_multi_client");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "multi-math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Create 3 separate clients
    let client1 = RpcClient::with_transport(transport.clone(), "client-1").await?;
    let client2 = RpcClient::with_transport(transport.clone(), "client-2").await?;
    let client3 = RpcClient::with_transport(transport.clone(), "client-3").await?;

    // All clients make requests concurrently
    let (r1, r2, r3) = tokio::join!(
        client1.request_to::<AddRequest, AddResponse>(
            "multi-math",
            "add",
            AddRequest { a: 1, b: 1 }
        ),
        client2.request_to::<AddRequest, AddResponse>(
            "multi-math",
            "add",
            AddRequest { a: 2, b: 2 }
        ),
        client3.request_to::<AddRequest, AddResponse>(
            "multi-math",
            "add",
            AddRequest { a: 3, b: 3 }
        ),
    );

    assert_eq!(r1?.sum, 2);
    assert_eq!(r2?.sum, 4);
    assert_eq!(r3?.sum, 6);

    server.shutdown().await?;
    transport.close().await?;
    handle.await.expect("server task panicked")?;
    Ok(())
}

#[tokio::test]
#[ignore] // TODO: Implement error response protocol
async fn test_transport_disconnect() -> Result<()> {
    #[cfg(feature = "logging")]
    init_logging();

    let config = RpcConfig::memory("test_disconnect");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "disconnect-math");

    server.register("add", |req: AddRequest| async move {
        // Slow handler to ensure request is in-flight
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = RpcClient::with_transport(transport.clone(), "disconnect-client").await?;

    // Start request
    let fut = client.request_to::<AddRequest, AddResponse>(
        "disconnect-math",
        "add",
        AddRequest { a: 1, b: 1 },
    );

    // Close transport while request is in-flight
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    transport.close().await?;

    // Request should fail
    let result = fut.await;
    assert!(result.is_err(), "expected error after transport disconnect");

    handle.await.expect("server task panicked")?;
    Ok(())
}

#[cfg(feature = "logging")]
mod imp {
    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn init() {
        INIT.call_once(|| {
            let _ = env_logger::builder().is_test(true).try_init();
        });
    }
}

#[cfg(not(feature = "logging"))]
mod imp {
    #[inline]
    pub fn init() {}
}

pub fn init_logging() {
    imp::init();
}
