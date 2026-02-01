# mqtt-rpc-rs

RPC semantics over MQTT pub/sub with automatic request/response correlation.

## Problem

MQTT is a lightweight pub/sub protocol widely used in IoT, but it lacks built-in RPC semantics. Every developer using MQTT for request/response patterns must solve the same problems:

- **Request/response correlation** - Matching responses to requests
- **Timeout handling** - Detecting when requests will never get responses
- **Concurrent request handling** - Processing multiple in-flight requests
- **Code duplication** - Reimplementing this logic in every project

This crate extracts that common pattern into a reusable library.

## Features

- ✅ **Async/await API** - Built on tokio and rumqttc
- ✅ **Automatic correlation** - UUID-based request/response matching
- ✅ **Concurrent requests** - Multiple in-flight requests per client
- ✅ **Timeout support** - Per-request timeout handling
- ✅ **MQTT v3.1.1 & v5 compatible** - JSON-encoded response topic
- ✅ **JSON serialization** - Serde-based, interop with other languages
- ✅ **Concurrent handlers** - Server handlers execute in parallel

## Quick Start

### Add to your project

```bash
cargo add mqtt-rpc-rs
```

### Client Example

```rust
use mqtt_rpc_rs::{RpcClient, Error};
use rumqttc::{MqttOptions, AsyncClient, EventLoop};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Deserialize)]
struct MathResponse {
    result: i32,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Create MQTT client
    let mut mqtt_options = MqttOptions::new("client-01", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));
    
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    
    // Spawn eventloop
    tokio::spawn(async move {
        loop {
            eventloop.poll().await.ok();
        }
    });
    
    // Create RPC client
    let rpc_client = RpcClient::new(mqtt_client, "client-01").await?;
    
    // Send request, wait for response
    let request = AddRequest { a: 5, b: 3 };
    let response: MathResponse = rpc_client
        .request("math/add", &request)
        .timeout(Duration::from_secs(5))
        .await?;
    
    println!("Result: {}", response.result);
    
    Ok(())
}
```

### Server Example

```rust
use mqtt_rpc_rs::{RpcServer, Error};
use rumqttc::{MqttOptions, AsyncClient, EventLoop};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Serialize)]
struct MathResponse {
    result: i32,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Create MQTT client
    let mut mqtt_options = MqttOptions::new("math-server", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));
    
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    
    // Spawn eventloop
    tokio::spawn(async move {
        loop {
            eventloop.poll().await.ok();
        }
    });
    
    // Create RPC server
    let mut server = RpcServer::new(mqtt_client).await?;
    
    // Register handler
    server.handle("math/add", |req: AddRequest| async move {
        Ok(MathResponse {
            result: req.a + req.b,
        })
    }).await?;
    
    // Run server
    server.run().await?;
    
    Ok(())
}
```

## How It Works

1. **Client** generates a UUID correlation ID and subscribes to its response topic
2. **Client** publishes request to `{topic}/request` with `response_topic` in JSON payload
3. **Server** receives request, spawns async handler (concurrent execution)
4. **Handler** completes, server publishes response to client's response topic (from JSON)
5. **Client** matches response by correlation ID and resolves the Future

See [docs/architecture.md](docs/architecture.md) for detailed design decisions.

## Requirements

- Rust 1.70+
- MQTT broker (mosquitto, EMQX, HiveMQ, etc.) - works with v3.1.1 and v5
- Tokio runtime

## Examples

Run the examples (requires mosquitto running on localhost:1883):

```bash
# Terminal 1: Start server
cargo run --example math_server

# Terminal 2: Send requests
cargo run --example math_client
```

## Testing

Run tests with a local mosquitto broker:

```bash
# Start mosquitto (or use scripts/start-mosquitto.sh)
mosquitto -v

# Run tests
cargo test
```

## Documentation

- [Architecture Design](docs/architecture.md) - Design decisions and wire protocol
- [API Documentation](https://docs.rs/mqtt-rpc-rs) - Generated docs (when published)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

MIT License - see [LICENSE](LICENSE) file.

## Roadmap

- [x] v0.1.0: Core RPC client/server with JSON-encoded response_topic
- [ ] v0.2.0: MQTT v5 native response_topic property (optional feature)
- [ ] v0.3.0: Metrics and observability hooks
- [ ] v0.4.0: Connection pooling and load balancing

## Why mqtt-rpc-rs?

Other MQTT RPC solutions use UUIDs for correlation IDs or wrap multiple layers. This crate provides:

- **Clean API** - Simple, ergonomic async/await interface
- **Direct rumqttc usage** - No extra abstraction layers
- **Broad compatibility** - Works with MQTT v3.1.1 and v5
- **Well-documented** - Clear wire protocol for interop

Perfect for edge computing, IoT command/control, and microservice communication over MQTT.
