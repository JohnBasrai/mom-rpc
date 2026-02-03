use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;

use mqtt_rpc_rs::{
    //
    create_transport,
    Result,
    RpcClient,
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
}

impl MathServer {
    // ---
    async fn new(id: &str) -> Result<Self> {
        // ---
        let transport = create_transport(id).await?;
        let server = RpcServer::new(transport.clone(), "math".to_owned());

        server.register("add", |req: AddRequest| async move {
            // ---
            Ok(AddResponse { sum: req.a + req.b })
        });
        let handle = server.run().await?;

        Ok(Self {
            handle,
            _server: server,
            transport,
        })
    }

    async fn shutdown(self) -> Result<()> {
        // --
        // async cleanup
        self.transport.close().await?;

        // JoinError -> panic, inner Result -> ?
        self.handle.await.expect("server task panicked")?;

        Ok(())
    }

    fn transport(&self) -> TransportPtr {
        self.transport.clone()
    }
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    log::info!("Starting basic request test");

    let server = MathServer::new("test_basic_request").await?;
    log::info!("after MathServer::new");

    let client = RpcClient::with_transport(server.transport(), "Sally".to_string()).await?;
    log::info!("after RpcClient::new");

    log::info!("sending math add 2 3...");
    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;
    log::info!("sending math add 2 3...done");

    assert_eq!(resp.sum, 5);

    log::info!("calling server shutdown");

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() {
    // ---
    #[cfg(feature = "logging")]
    init_logging();

    let server = MathServer::new("test_concurrent_requests").await.unwrap();
    let client = RpcClient::with_transport(server.transport(), "George".to_string())
        .await
        .unwrap();

    let mut handles = Vec::new();

    for i in 0..10 {
        // ---
        let c = client.clone();

        handles.push(tokio::spawn(async move {
            let resp: AddResponse = c
                .request_to("math", "add", AddRequest { a: i, b: i })
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

    let transport = create_transport("test_timeout").await?;
    let server = RpcServer::new(transport.clone(), "lazy-math".to_owned());
    let handle = server.run().await?;

    server.register("add", |req: AddRequest| async move {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        Ok(AddResponse { sum: req.a + req.b })
    });

    let client = RpcClient::with_transport(transport.clone(), "Denis".to_string()).await?;

    log::info!("test_timeout: sending 1 + 1");

    let fut =
        client.request_to::<AddRequest, AddResponse>("math", "add", AddRequest { a: 1, b: 1 });

    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;

    assert!(res.is_err(), "request unexpectedly completed");

    log::info!("test_timeout: {:?}", res);

    log::info!("test_timeout: closing transport");
    transport.close().await?;

    log::info!("test_timeout: join handler");
    handle.await.expect("test_timeout:: server task panicked")?;

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
