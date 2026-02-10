// tests/transport_memory.rs

use bytes::Bytes;
use tokio::time::{timeout, Duration};

use mom_rpc::{
    // ---
    Address,
    CorrelationId,
    Envelope,
    RpcConfig,
};

#[tokio::test]
async fn memory_subscribe_then_publish_delivers() {
    // ---
    // Arrange
    // ---
    let config = RpcConfig::memory("mstpd");

    let transport = mom_rpc::create_memory_transport(&config)
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
