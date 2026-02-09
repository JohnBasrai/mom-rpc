//! Math RPC example using in-memory transport.
//!
//! Demonstrates both client and server in a single process using MemoryTransport.
//! This is useful for testing and single-process applications.
//!
//! Run with: cargo run --example math_memory

mod common;

use common::{AddRequest, AddResponse};
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig, RpcServer};

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    env_logger::init();
    let config = RpcConfig::memory("math");

    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "math");

    server.register("add", |req: AddRequest| async move {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        Ok(AddResponse { sum: req.a + req.b })
    });

    // Spawn server in background so we can use the main task for client
    let _handle = server.spawn();

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 20, b: 3 })
        .await?;

    println!("20 + 3 = {}", resp.sum);

    // Clean shutdown
    server.shutdown().await?;
    transport.close().await?;
    Ok(())
}
