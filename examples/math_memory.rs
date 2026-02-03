use mqtt_rpc_rs::{create_transport, RpcClient, RpcServer};
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
    let transport = create_transport("loopback").await?;

    let server = RpcServer::new(transport.clone(), "math".to_owned());

    server.register("add", |req: AddRequest| async move {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        Ok(AddResponse { sum: req.a + req.b })
    });

    let _handle = server.run().await?;

    let client = RpcClient::with_transport(transport.clone(), "Roxy".to_string()).await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 20, b: 3 })
        .await?;

    println!("20 + 3 = {}", resp.sum);

    transport.close().await?;
    Ok(())
}
