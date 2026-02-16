#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn
)]

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;

use tracing::info;

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
    config: RpcConfig,
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
            config,
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

    fn config(&self) -> &RpcConfig {
        &self.config
    }
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    init_tracing();

    info!("Starting basic request test");

    let server = MathServer::new("test_basic_request").await?;
    info!("after MathServer::new");

    let client =
        RpcClient::with_transport(server.transport(), "Sally", server.config().clone()).await?;
    info!("after RpcClient::new");

    info!("sending math add 2 3...");
    let resp: AddResponse = client
        .request_to(server.node_id(), "add", AddRequest { a: 2, b: 3 })
        .await?;
    info!("sending math add 2 3...done");

    assert_eq!(resp.sum, 5);

    info!("calling server shutdown");

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() {
    // ---
    init_tracing();

    let server = MathServer::new("test_concurrent_requests").await.unwrap();
    let client = RpcClient::with_transport(server.transport(), "George", server.config().clone())
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
    init_tracing();

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

    let client = RpcClient::with_transport(transport.clone(), "Denis", config.clone()).await?;

    info!("test_timeout: sending 1 + 1");

    let fut =
        client.request_to::<AddRequest, AddResponse>("lazy-math", "add", AddRequest { a: 1, b: 1 });

    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;

    assert!(res.is_err(), "request unexpectedly completed");

    info!("test_timeout: {:?}", res);

    info!("test_timeout: shutting down server");
    server.shutdown().await?;

    info!("test_timeout: closing transport");
    transport.close().await?;

    info!("test_timeout: join handler");
    handle.await.expect("test_timeout:: server task panicked")?;

    Ok(())
}

#[tokio::test]
#[ignore] // TODO: Implement error response protocol
async fn test_error_response() -> Result<()> {
    // --
    init_tracing();

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

    let client =
        RpcClient::with_transport(transport.clone(), "error-client", config.clone()).await?;

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
    // ---
    init_tracing();

    let config = RpcConfig::memory("test_multi_client");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "multi-math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Create 3 separate clients
    let client1 = RpcClient::with_transport(transport.clone(), "client-1", config.clone()).await?;
    let client2 = RpcClient::with_transport(transport.clone(), "client-2", config.clone()).await?;
    let client3 = RpcClient::with_transport(transport.clone(), "client-3", config.clone()).await?;

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
    // ---
    init_tracing();

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

    let client =
        RpcClient::with_transport(transport.clone(), "disconnect-client", config.clone()).await?;

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

#[tokio::test]
async fn test_request_with_timeout_success() -> Result<()> {
    let config = RpcConfig::memory("test_timeout_success");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "timeout-math");

    server.register("add", |req: AddRequest| async move {
        // Fast handler - should complete before timeout
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client =
        RpcClient::with_transport(transport.clone(), "timeout-client", config.clone()).await?;

    // Request with generous timeout should succeed
    let resp: AddResponse = client
        .request_with_timeout(
            "timeout-math",
            "add",
            AddRequest { a: 5, b: 3 },
            Duration::from_secs(1),
        )
        .await?;

    assert_eq!(resp.sum, 8);

    server.shutdown().await?;
    transport.close().await?;
    handle.await.expect("server task panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_request_with_timeout_expires() -> Result<()> {
    let config = RpcConfig::memory("test_timeout_expires");
    let transport = create_memory_transport(&config).await.unwrap();
    let server = RpcServer::with_transport(transport.clone(), "slow-math");

    server.register("add", |req: AddRequest| async move {
        // Slow handler - intentionally longer than timeout
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(AddResponse { sum: req.a + req.b })
    });

    let handle = server.spawn();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = RpcClient::with_transport(transport.clone(), "timeout-client-2", config.clone())
        .await
        .unwrap();

    // Request with very short timeout should fail
    let result: Result<AddResponse> = client
        .request_with_timeout(
            "slow-math",
            "add",
            AddRequest { a: 5, b: 3 },
            Duration::from_millis(50), // Timeout before handler completes
        )
        .await;

    match result {
        Err(RpcError::Timeout) => {
            // Expected - timeout occurred
        }
        Ok(_) => {
            eprintln!("expected timeout but request succeeded");
            return Err(RpcError::InvalidResponse);
        }
        Err(e) => {
            println!("expected Timeout error but got: {e}");
            return Err(RpcError::InvalidResponse);
        }
    }

    server.shutdown().await?;
    transport.close().await?;
    handle.await.expect("server task panicked")?;
    Ok(())
}

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[tokio::test]
async fn test_run_blocks_until_shutdown() -> Result<()> {
    // ---
    let config = RpcConfig::memory("test_run_blocks");
    let transport = create_memory_transport(&config).await?;
    let server = RpcServer::with_transport(transport.clone(), "run-math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    let shutdown_called = Arc::new(AtomicBool::new(false));
    let shutdown_called_clone = shutdown_called.clone();
    let server_clone = server.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown_called_clone.store(true, Ordering::SeqCst);
        server_clone.shutdown().await.expect("shutdown failed");
    });

    server.run().await?;

    assert!(
        shutdown_called.load(Ordering::SeqCst),
        "run() returned before shutdown() was called"
    );

    transport.close().await?;
    Ok(())
}

use std::sync::Once;

static INIT: Once = Once::new();

fn init_tracing() {
    // ---
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_line_number(true)
            .with_ansi(false)
            .try_init();
    });
}
