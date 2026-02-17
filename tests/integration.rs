#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn
)]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use tracing::info;

use mom_rpc::{
    // ---
    MemoryHub,
    Result,
    RpcBroker,
    RpcBrokerBuilder,
    RpcError,
    TransportBuilder,
    TransportConfig,
    TransportMode,
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

/// Test fixture: a server broker on an isolated [`MemoryHub`].
///
/// Each `MathServer` owns the hub so tests are fully isolated from each other.
struct MathServer {
    // ---
    _handle: JoinHandle<()>,
    broker: RpcBroker,
    node_id: String,
    hub: Arc<MemoryHub>,
}

impl MathServer {
    // ---
    async fn new(id: &str) -> Result<Self> {
        // ---
        let node_id = format!("math-{id}");
        let hub = MemoryHub::new();

        let transport = server_transport(&node_id, hub.clone()).await?;
        let broker = RpcBrokerBuilder::new(transport).build()?;

        broker.register("add", |req: AddRequest| async move {
            // ---
            Ok(AddResponse { sum: req.a + req.b })
        })?;

        let handle = broker.clone().spawn()?;

        // Give the server task time to subscribe before returning
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(Self {
            _handle: handle,
            broker,
            node_id,
            hub,
        })
    }

    async fn shutdown(self) -> Result<()> {
        // ---
        self.broker.shutdown().await;
        Ok(())
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn hub(&self) -> Arc<MemoryHub> {
        self.hub.clone()
    }
}

/// Create a server-mode transport on the given hub.
async fn server_transport(node_id: &str, hub: Arc<MemoryHub>) -> Result<TransportPtr> {
    // ---
    mom_rpc::create_memory_transport_with_hub(
        TransportConfig {
            uri: String::new(),
            node_id: node_id.into(),
            mode: TransportMode::Server,
            request_queue: Some(format!("requests/{node_id}")),
            response_queue: None,
            transport_type: None,
            keep_alive_secs: None,
        },
        hub,
    )
    .await
}

/// Create a client-mode transport on the given hub.
async fn client_transport(node_id: &str, hub: Arc<MemoryHub>) -> Result<TransportPtr> {
    // ---
    mom_rpc::create_memory_transport_with_hub(
        TransportConfig {
            uri: String::new(),
            node_id: node_id.into(),
            mode: TransportMode::Client,
            request_queue: None,
            response_queue: Some(format!("responses/{node_id}")),
            transport_type: None,
            keep_alive_secs: None,
        },
        hub,
    )
    .await
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    init_tracing();
    info!("Starting basic request test");

    let server = MathServer::new("test_basic_request").await?;
    let client_transport = client_transport("Sally", server.hub()).await?;
    let client = RpcBrokerBuilder::new(client_transport).build()?;

    info!("sending math add 2 3...");
    let resp: AddResponse = client
        .request_to(server.node_id(), "add", AddRequest { a: 2, b: 3 })
        .await?;
    info!("sending math add 2 3...done");

    assert_eq!(resp.sum, 5);
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() -> Result<()> {
    // ---
    init_tracing();

    let server = MathServer::new("test_concurrent_requests").await?;
    let node_id = server.node_id().to_string();
    let mut handles = Vec::new();

    for i in 0..10 {
        // ---
        let transport = client_transport(&format!("client-{i}"), server.hub()).await?;
        let client = RpcBrokerBuilder::new(transport).build()?;
        let node_id = node_id.clone();

        handles.push(tokio::spawn(async move {
            let resp: AddResponse = client
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

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_timeout() -> Result<()> {
    // ---
    init_tracing();

    let hub = MemoryHub::new();
    let server_node = "lazy-math";
    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    server.register("add", |req: AddRequest| async move {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        Ok(AddResponse { sum: req.a + req.b })
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("Denis", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    info!("test_timeout: sending 1 + 1");
    let fut =
        client.request_to::<AddRequest, AddResponse>(server_node, "add", AddRequest { a: 1, b: 1 });
    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;
    assert!(res.is_err(), "request unexpectedly completed");
    info!("test_timeout: {:?}", res);

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
#[ignore] // TODO: Implement error response protocol
async fn test_error_response() -> Result<()> {
    // ---
    init_tracing();

    let hub = MemoryHub::new();
    let server_node = "error-math";
    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    server.register("divide", |req: AddRequest| async move {
        if req.b == 0 {
            return Err(RpcError::InvalidRequest);
        }
        Ok(AddResponse { sum: req.a / req.b })
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("error-client", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    let result: Result<AddResponse> = client
        .request_to(server_node, "divide", AddRequest { a: 10, b: 0 })
        .await;
    assert!(result.is_err(), "expected error for division by zero");

    let resp: AddResponse = client
        .request_to(server_node, "divide", AddRequest { a: 10, b: 2 })
        .await?;
    assert_eq!(resp.sum, 5);

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn test_multiple_clients() -> Result<()> {
    // ---
    init_tracing();

    let server = MathServer::new("test_multiple_clients").await?;
    let node_id = server.node_id().to_string();

    let t1 = client_transport("client-1", server.hub()).await?;
    let t2 = client_transport("client-2", server.hub()).await?;
    let t3 = client_transport("client-3", server.hub()).await?;

    let client1 = RpcBrokerBuilder::new(t1).build()?;
    let client2 = RpcBrokerBuilder::new(t2).build()?;
    let client3 = RpcBrokerBuilder::new(t3).build()?;

    let (r1, r2, r3) = tokio::join!(
        client1.request_to::<AddRequest, AddResponse>(&node_id, "add", AddRequest { a: 1, b: 1 }),
        client2.request_to::<AddRequest, AddResponse>(&node_id, "add", AddRequest { a: 2, b: 2 }),
        client3.request_to::<AddRequest, AddResponse>(&node_id, "add", AddRequest { a: 3, b: 3 }),
    );

    assert_eq!(r1?.sum, 2);
    assert_eq!(r2?.sum, 4);
    assert_eq!(r3?.sum, 6);

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_request_with_timeout_success() -> Result<()> {
    // ---
    let server = MathServer::new("test_timeout_success").await?;
    let node_id = server.node_id().to_string();

    let transport = client_transport("timeout-client", server.hub()).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    let resp: AddResponse = client
        .request_to_with_timeout(
            &node_id,
            "add",
            AddRequest { a: 5, b: 3 },
            Duration::from_secs(1),
        )
        .await?;

    assert_eq!(resp.sum, 8);
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_request_with_timeout_expires() -> Result<()> {
    // ---
    let hub = MemoryHub::new();
    let server_node = "slow-math";

    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    server.register("add", |req: AddRequest| async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(AddResponse { sum: req.a + req.b })
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("timeout-client-2", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    let result: Result<AddResponse> = client
        .request_to_with_timeout(
            server_node,
            "add",
            AddRequest { a: 5, b: 3 },
            Duration::from_millis(50),
        )
        .await;

    match result {
        Err(RpcError::Timeout) => {}
        Ok(_) => panic!("expected timeout but request succeeded"),
        Err(e) => panic!("expected Timeout error but got: {e}"),
    }

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn test_run_blocks_until_shutdown() -> Result<()> {
    // ---
    use std::sync::atomic::{AtomicBool, Ordering};

    let hub = MemoryHub::new();
    let node_id = "run-math";
    let transport = server_transport(node_id, hub).await?;
    let broker = RpcBrokerBuilder::new(transport).build()?;

    broker.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    })?;

    let shutdown_called = Arc::new(AtomicBool::new(false));
    let shutdown_called_clone = shutdown_called.clone();
    let broker_clone = broker.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown_called_clone.store(true, Ordering::SeqCst);
        broker_clone.shutdown().await;
    });

    broker.run().await?;

    assert!(
        shutdown_called.load(Ordering::SeqCst),
        "run() returned before shutdown() was called"
    );

    Ok(())
}

/// Verify that TransportBuilder without an explicit transport_type falls back to
/// memory transport when no broker transport features are enabled.
#[tokio::test]
async fn test_transport_builder_fallback_to_memory() -> Result<()> {
    // ---
    // With no transport_type set, the builder tries dust_dds → rumqttc → lapin → memory.
    // In the default feature set (no broker transports enabled), all three broker
    // factories return Err immediately via Null Object stubs, so memory is used.
    let transport = TransportBuilder::new()
        .uri("memory://")
        .node_id("fallback-test")
        .full_duplex()
        .build()
        .await?;

    // Verify we got a working memory transport by doing a basic pub/sub round-trip
    use bytes::Bytes;
    use mom_rpc::{Address, Envelope, Subscription};

    let sub = transport
        .subscribe(Subscription::from("fallback/test"))
        .await?;

    let env = Envelope::response(
        Address::from("fallback/test"),
        Bytes::from_static(b"hello"),
        "corr-1".into(),
        "application/json".into(),
    );
    transport.publish(env).await?;

    let received = tokio::time::timeout(Duration::from_millis(100), {
        let mut sub = sub;
        async move { sub.inbox.recv().await }
    })
    .await
    .expect("timed out")
    .expect("channel closed");

    assert_eq!(received.payload, Bytes::from_static(b"hello"));
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
