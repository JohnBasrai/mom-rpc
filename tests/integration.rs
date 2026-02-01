//! Integration tests for mqtt-rpc-rs
//!
//! These tests require a running mosquitto broker on localhost:1883

use mqtt_rpc_rs::{Error, RpcClient, RpcServer};
use rumqttc::{AsyncClient, MqttOptions};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestRequest {
    // ---
    value: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestResponse {
    // ---
    result: i32,
}

async fn setup_mqtt_client(client_id: &str) -> (AsyncClient, tokio::task::JoinHandle<()>) {
    // ---
    let mut mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

    let handle = tokio::spawn(async move {
        loop {
            if eventloop.poll().await.is_err() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    });

    // Give eventloop time to connect
    tokio::time::sleep(Duration::from_millis(200)).await;

    (client, handle)
}

#[tokio::test]
async fn test_basic_request_response() -> Result<(), Error> {
    // ---
    // Setup server
    let (server_mqtt, _server_handle) = setup_mqtt_client("test-server-01").await;
    let mut server = RpcServer::new(server_mqtt).await?;

    server
        .handle("test/echo", |req: TestRequest| async move {
            Ok(TestResponse {
                result: req.value * 2,
            })
        })
        .await?;

    // Setup client
    let (client_mqtt, _client_handle) = setup_mqtt_client("test-client-01").await;
    let client = RpcClient::new(client_mqtt, "test-client-01").await?;

    // Send request
    let request = TestRequest { value: 21 };
    let response: TestResponse = client
        .request_timeout("test/echo", &request, Some(Duration::from_secs(5)))
        .await?;

    assert_eq!(response.result, 42);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() -> Result<(), Error> {
    // ---
    // Setup server
    let (server_mqtt, _server_handle) = setup_mqtt_client("test-server-02").await;
    let mut server = RpcServer::new(server_mqtt).await?;

    server
        .handle("test/double", |req: TestRequest| async move {
            // Simulate some processing time
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(TestResponse {
                result: req.value * 2,
            })
        })
        .await?;

    // Setup client
    let (client_mqtt, _client_handle) = setup_mqtt_client("test-client-02").await;
    let client = RpcClient::new(client_mqtt, "test-client-02").await?;

    // Send multiple concurrent requests
    let mut handles = vec![];
    for i in 1..=5 {
        let client = &client;
        let handle = tokio::spawn(async move {
            let request = TestRequest { value: i };
            let response: TestResponse = client
                .request_timeout("test/double", &request, Some(Duration::from_secs(5)))
                .await?;
            Ok::<_, Error>((i, response.result))
        });
        handles.push(handle);
    }

    // Wait for all responses
    for handle in handles {
        let (input, output) = handle.await.unwrap()?;
        assert_eq!(output, input * 2);
    }

    Ok(())
}

#[tokio::test]
async fn test_timeout() -> Result<(), Error> {
    // ---
    // Setup server that doesn't respond
    let (server_mqtt, _server_handle) = setup_mqtt_client("test-server-03").await;
    let mut server = RpcServer::new(server_mqtt).await?;

    server
        .handle("test/slow", |_req: TestRequest| async move {
            // Never respond
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(TestResponse { result: 0 })
        })
        .await?;

    // Setup client
    let (client_mqtt, _client_handle) = setup_mqtt_client("test-client-03").await;
    let client = RpcClient::new(client_mqtt, "test-client-03").await?;

    // Send request with short timeout
    let request = TestRequest { value: 1 };
    let result: Result<TestResponse, _> = client
        .request_timeout("test/slow", &request, Some(Duration::from_millis(100)))
        .await;

    assert!(matches!(result, Err(Error::Timeout)));

    Ok(())
}
