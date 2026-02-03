
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use mqtt_rpc_rs::{RpcClient, RpcClientBuilder, RpcConfig, RpcServer, RpcServerBuilder};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse {
    sum: i32,
}

async fn start_math_server(node_id: &str) -> Result<RpcServer> {
    // ---
    let config = RpcConfig::new("mqtt://localhost:1883").with_keep_alive_secs(5);

    let server = RpcServerBuilder::new(node_id, config).start().await?;

    server.register("add", |bytes| {
        // ---
        let req: AddRequest = serde_json::from_slice(&bytes).expect("failed to decode request");

        let resp = AddResponse { sum: req.a + req.b };

        serde_json::to_vec(&resp).expect("failed to encode response")
    });

    Ok(server)
}

async fn start_client(node_id: &str) -> Result<RpcClient> {
    // ---
    let config = RpcConfig::new("mqtt://localhost:1883").with_keep_alive_secs(5);
    RpcClientBuilder::new(node_id, config).start().await
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    let _server = start_math_server("test-server-basic").await?;
    let client = start_client("test-client-basic").await?;

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let resp: AddResponse = client
        .request_to("test-server-basic", "add", AddRequest { a: 2, b: 3 })
        .await?;

    assert_eq!(resp.sum, 5);
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    let _server = start_math_server("test-server-concurrent").await?;
    let client = start_client("test-client-concurrent").await?;

    let mut handles = Vec::new();

    std::thread::sleep(std::time::Duration::from_millis(1000));

    for i in 0..10 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let resp: AddResponse = c
                .request_to("test-server-concurrent", "add", AddRequest { a: i, b: i })
                .await
                .unwrap();
            resp.sum
        }));
    }

    for (i, h) in handles.into_iter().enumerate() {
        let sum = h.await?;
        assert_eq!(sum, (i as i32) * 2);
    }

    Ok(())
}

#[tokio::test]
async fn test_timeout() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    // Server exists but does not register a handler.
    let config = RpcConfig::new("mqtt://localhost:1883").with_keep_alive_secs(5);
    let _server = RpcServerBuilder::new("test-server-timeout", config)
        .start()
        .await?;

    let client = start_client("test-client-timeout").await?;

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let fut = client.request_to::<AddRequest, AddResponse>(
        "test-server-timeout",
        "add",
        AddRequest { a: 1, b: 1 },
    );

    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;

    assert!(res.is_err(), "request unexpectedly completed");

    Ok(())
}

#[cfg(feature = "logging")]
fn init_logging() {
    // ---
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Debug)
            .init();
    });
}
