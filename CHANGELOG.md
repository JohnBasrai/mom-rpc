### Changelog (what changed)

* **Public API no longer exposes `rumqttc`**: both `RpcServer` and `RpcClient` now start from `builder(node_id, RpcConfig)` and **own their internal event loop**.
* Added **`RpcConfig`** (`broker_addr`, `keep_alive_secs`) and pushed all MQTT option building inside the crate.
* Introduced a **generic `RpcBuilder<TKind>` + `RpcKind` specialization hook**, used by both server + client to share the “start transport + spawn event loop” lifecycle code.
* Refactored **client to mirror server lifecycle**: `RpcClientBuilder::start().await -> RpcClient`, internal subscribe to `responses/{node_id}`, internal dispatch completing pending requests.
* Updated **integration tests + math examples** to the new API (no manual eventloop spawning).

### Notes (scope)

You asked for `RpcConfig` under `src/config/rpc_config.rs` and a shared builder under `src/rpc_builder.rs`, so the tar includes those **new files** in addition to the 6 you listed.
