//! MQTT transport implementation using `mqtt_async_client`.
//!
//! This module provides an implementation of the `Transport` trait backed by
//! an MQTT broker connection. It follows an **actor-based concurrency model**
//! to safely integrate with the underlying MQTT client, which is `Send` but
//! explicitly **not `Sync`** and requires mutable access for receiving messages.
//!
//! ## Concurrency model
//!
//! - A single background **actor task** owns the MQTT `Client`.
//! - The actor is responsible for:
//!   - publishing outbound messages,
//!   - registering broker subscriptions,
//!   - reading inbound publishes via `read_subscriptions()`,
//!   - clean shutdown of the connection.
//! - All interaction with the MQTT client is serialized through this actor;
//!   no other task ever touches the client directly.
//!
//! This design preserves the public `Transport` contract (`Send + Sync`) while
//! respecting the MQTT client’s internal mutability and state-machine semantics.
//!
//! ## Message delivery semantics
//!
//! Incoming MQTT publishes are **demultiplexed by topic** and **fanned out**
//! to all local subscribers registered for that topic, matching the memory
//! transport contract:
//!
//! - Fanout delivers messages to *all* subscribers.
//! - Delivery is best-effort and non-durable.
//! - There is no replay, persistence, or retained-message support.
//!
//! Each call to `subscribe()` registers a new local inbox channel. Multiple
//! subscribers for the same topic are supported.
//!
//! ## Scope and limitations
//!
//! - One transport instance corresponds to a single broker connection.
//! - The transport assumes a small number of active topics and subscribers
//!   (typical RPC-style usage).
//! - Text-based payloads (JSON) are decoded in the actor before fanout; payload
//!   parsing cost dominates over synchronization overhead.
//!
//! This module intentionally avoids exposing MQTT-specific concepts (QoS,
//! retain flags, session state) outside the transport boundary.
//! MQTT transport implementation using `mqtt_async_client`.
//!
//! See module-level documentation for architecture and concurrency semantics.

use mqtt_async_client::client::{
    //
    Client,
    KeepAlive,
    Publish,
    QoS,
    Subscribe,
    SubscribeTopic,
};

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;

use crate::{
    //
    Envelope,
    PublishOptions,
    Result,
    RpcConfig,
    RpcError,
    SubscribeOptions,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};

//
// Logging macros (transport-local for now; intended to be shared later)
//

macro_rules! log_debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::debug!($($arg)*);
    };
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::error!($($arg)*);
    };
}

type SubscriberMap = Arc<RwLock<HashMap<String, Vec<mpsc::Sender<Envelope>>>>>;
type TaskList = Arc<RwLock<Vec<JoinHandle<()>>>>;

//
// Actor commands
//

enum Cmd {
    Publish {
        env: Envelope,
        resp: oneshot::Sender<Result<()>>,
    },
    Subscribe {
        topic: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Close {
        resp: oneshot::Sender<Result<()>>,
    },
}

enum ActorStep {
    Continue,
    Stop,
}

impl Cmd {
    // ---

    /// Dispatches an actor command to the correct handler on the actor
    async fn handle(self, actor: &mut MqttActor) -> ActorStep {
        // ---

        match self {
            Cmd::Publish { env, resp } => {
                actor.handle_publish(env).await;
                let _ = resp.send(Ok(()));
                ActorStep::Continue
            }
            Cmd::Subscribe { topic, resp } => {
                actor.handle_subscribe(topic).await;
                let _ = resp.send(Ok(()));
                ActorStep::Continue
            }
            Cmd::Close { resp } => {
                actor.handle_close().await;
                let _ = resp.send(Ok(()));
                ActorStep::Stop
            }
        }
    }
}

/// MQTT-based implementation of the `Transport` trait.
///
/// Represents a single broker connection and provides best-effort,
/// non-durable message delivery consistent with memory transport semantics.
pub struct MqttAsyncClientTransport {
    transport_id: String,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl MqttAsyncClientTransport {
    //---

    pub fn creat(transport_id: impl Into<String>, client: Client) -> TransportPtr {
        // ---
        let transport_id = transport_id.into();

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let subscribers = Arc::new(RwLock::new(HashMap::new()));
        let tasks = Arc::new(RwLock::new(Vec::new()));

        let actor = MqttActor {
            _transport_id: transport_id.clone(),
            client,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
        };

        let handle = tokio::spawn(actor.run());
        let tasks_clone = Arc::clone(&tasks);

        tokio::spawn(async move {
            tasks_clone.write().await.push(handle);
        });

        Arc::new(Self {
            transport_id,
            cmd_tx,
            subscribers,
            tasks,
        })
    }
}

struct MqttActor {
    _transport_id: String, // for logging only
    client: Client,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
}

impl MqttActor {
    // ---

    async fn run(mut self) {
        // ---

        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            if matches!(cmd.handle(&mut self).await, ActorStep::Stop) {
                                break;
                            }
                        }
                        None => break,
                    }
                }

                read = self.client.read_subscriptions() => {
                    match read {
                        Ok(rr) => {
                            self.handle_incoming(rr.topic(), rr.payload()).await;
                        }
                        Err(_err) => {
                            log_debug!(
                                "{}: error reading subscription: {_err}",
                                self._transport_id
                            );
                        }
                    }
                }
            }
        }
    }

    async fn handle_publish(&mut self, env: Envelope) {
        // ---
        let topic = env.address.0.as_ref();

        let payload = match serde_json::to_vec(&env) {
            Ok(p) => p,
            Err(_err) => {
                log_error!(
                    "{}: failed to serialize publish payload: {_err}",
                    self._transport_id
                );
                return;
            }
        };

        let mut publish = Publish::new(topic.into(), payload);
        publish.set_qos(QoS::AtMostOnce).set_retain(false);

        if let Err(_err) = self.client.publish(&publish).await {
            log_error!(
                "{}: publish failed for topic {topic}: {_err}",
                self._transport_id
            );
        }
    }

    async fn handle_subscribe(&mut self, topic: String) {
        // ---
        let subscribe = Subscribe::new(vec![SubscribeTopic {
            topic_path: topic.clone(),
            qos: QoS::AtMostOnce,
        }]);

        if let Err(_err) = self.client.subscribe(subscribe).await {
            log_error!(
                "{}: broker subscribe failed for topic {topic}: {_err}",
                self._transport_id
            );
        }
    }

    async fn handle_close(&mut self) {
        // ---
        log_debug!("{}: disconnecting mqtt client", self._transport_id);

        if let Err(_err) = self.client.disconnect().await {
            log_debug!("{}: mqtt disconnect failed: {_err}", self._transport_id);
        }
    }

    async fn handle_incoming(&self, topic: &str, payload: &[u8]) {
        // ---
        let env = match serde_json::from_slice::<Envelope>(payload) {
            Ok(env) => env,
            Err(_err) => {
                log_debug!(
                    "{}: invalid envelope on topic {topic}: {_err}",
                    self._transport_id
                );
                return;
            }
        };

        let senders = {
            let map = self.subscribers.read().await;
            map.get(topic).cloned()
        };

        let Some(senders) = senders else {
            // No subscribers for this topic
            return;
        };

        // Snapshot the original subscriber count before consuming `senders`.
        // We move `senders` below to retain ownership of live senders, so we
        // must capture any metadata we still need up front.
        let original_len = senders.len();

        // Collect only the surviving (live) subscribers.
        // `Sender` is cheap to move/clone; we rebuild the list to evict
        // closed or backpressured subscribers.
        let mut survivors = Vec::with_capacity(original_len);

        // Consume `senders` so we keep owned `Sender` values.
        // Iterating by value avoids storing references back into the map.
        for tx in senders {
            match tx.try_send(env.clone()) {
                Ok(()) => {
                    // Subscriber is alive and accepted the message.
                    survivors.push(tx);
                }
                Err(_) => {
                    // Channel is full or receiver was dropped; evict.
                    // Best-effort delivery: slow or dead subscribers are removed.
                }
            }
        }

        // Only update the map if something changed.
        // This avoids unnecessary write-lock contention on the subscriber map.
        if survivors.len() != original_len {
            let mut map = self.subscribers.write().await;
            map.insert(topic.to_string(), survivors);
        }
    } // handle_incoming
}

#[async_trait::async_trait]
impl Transport for MqttAsyncClientTransport {
    // ---

    fn transport_id(&self) -> &str {
        &self.transport_id
    }

    async fn publish(&self, env: Envelope, _opts: PublishOptions) -> Result<()> {
        // ---
        let (tx, rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Publish { env, resp: tx })
            .await
            .map_err(|_| RpcError::Transport)?;

        rx.await.map_err(|_| RpcError::Transport)?
    }

    async fn subscribe(
        &self,
        sub: Subscription,
        _opts: SubscribeOptions,
    ) -> Result<SubscriptionHandle> {
        // ---

        let topic = sub.0.as_ref().to_string();

        let (tx, rx) = mpsc::channel(16);
        {
            let mut map = self.subscribers.write().await;
            map.entry(topic.clone()).or_default().push(tx);
        }

        let (resp_tx, resp_rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Subscribe {
                topic,
                resp: resp_tx,
            })
            .await
            .map_err(|_| RpcError::Transport)?;

        resp_rx.await.map_err(|_| RpcError::Transport)??;

        Ok(SubscriptionHandle { inbox: rx })
    }

    async fn close(&self) -> Result<()> {
        // ---

        let (tx, rx) = oneshot::channel();

        let _ = self.cmd_tx.send(Cmd::Close { resp: tx }).await;
        let _ = rx.await;

        let mut tasks = self.tasks.write().await;
        while let Some(handle) = tasks.pop() {
            let _ = handle.await;
        }

        Ok(())
    }
}

pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    let client = create_mqtt_client(config).await?;
    Ok(MqttAsyncClientTransport::creat("mqtt-async-client", client))
}

async fn create_mqtt_client(config: &RpcConfig) -> Result<Client> {
    // ---
    let broker_addr = &config.broker_addr;
    let client_id = &config.transport_id;

    let mut builder = Client::builder();
    builder.set_url_string(broker_addr).map_err(|_err| {
        log_error!("mqtt: failed to set broker URL {}: {_err}", broker_addr);
        RpcError::Transport
    })?;

    builder.set_client_id(Some(client_id.clone()));

    if let Some(keep_alive_secs) = config.keep_alive_secs {
        builder.set_keep_alive(KeepAlive::Enabled {
            secs: keep_alive_secs,
        });
    }

    let mut client = builder.build().map_err(|_err| {
        log_error!("mqtt: failed to build client: {_err}");
        RpcError::Transport
    })?;

    client.connect().await.map_err(|_err| {
        log_error!("mqtt: failed to connect to broker {}: {_err}", broker_addr);
        RpcError::Transport
    })?;

    Ok(client)
}
