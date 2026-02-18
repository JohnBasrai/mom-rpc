// tests/transport_memory.rs

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn
)]

use bytes::Bytes;
use tokio::time::{timeout, Duration};

use mom_rpc::{
    // ---
    Address,
    CorrelationId,
    Envelope,
    MemoryHub,
    TransportConfig,
    TransportMode,
};

#[tokio::test]
async fn memory_subscribe_then_publish_delivers() {
    // ---
    // Arrange
    // ---
    let hub = MemoryHub::new();

    let config = TransportConfig {
        uri: String::new(),
        node_id: "mstpd".into(),
        mode: TransportMode::FullDuplex,
        request_queue: None,
        response_queue: None,
        transport_type: None,
        keep_alive_secs: None,
    };

    let transport = mom_rpc::create_memory_transport_with_hub(config, hub)
        .await
        .expect("failed to create memory transport");

    let address = Address::from("test.address");

    let mut sub = transport
        .subscribe(address.clone().into())
        .await
        .expect("subscribe failed");

    let payload = Bytes::from_static(b"hello");

    let env = Envelope::request(
        address.clone(),
        "".into(),
        payload.clone(),
        CorrelationId::generate().as_str().into(),
        Address::from("tbd"),
        "application/json".into(),
    );

    // ---
    // Act
    // ---
    transport.publish(env).await.expect("publish failed");

    // ---
    // Assert
    // ---
    let received = timeout(Duration::from_millis(100), sub.inbox.recv())
        .await
        .expect("timed out waiting for message")
        .expect("subscription channel closed unexpectedly");

    assert_eq!(received.payload, payload);
    assert_eq!(received.address, address);
}
