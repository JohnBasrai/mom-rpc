//! Math RPC server example.
//!
//! NOTE:
//! This example is intended for brokered transports (MQTT, RabbitMQ, etc).
//! It cannot be used with MemoryTransport, which is in-process only.
//!
//! Run with: cargo run --example math_server
//!
//! Requires: a broker to be running

use mqtt_rpc_rs::{create_transport, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse {
    sum: i32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---
    // Transport will eventually be MQTT / Rabbit / etc.
    let transport = create_transport("broker").await?;

    let server = RpcServer::new(transport.clone(), "math".to_owned());

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    // Run until externally terminated
    let handle = server.run().await?;

    // Block forever (or until signal handling is added later)
    futures::future::pending::<()>().await;

    transport.close().await?;
    handle.await??;

    Ok(())
}
