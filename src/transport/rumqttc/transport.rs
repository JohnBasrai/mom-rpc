//! MQTT transport implementation using `rumqttc`.
//!
//! This module provides an implementation of the `Transport` trait backed by
//! an MQTT broker connection. It follows an **actor-based concurrency model**
//! to safely integrate with the underlying MQTT client.
//!
//! ## Concurrency model
//!
//! - A single background **actor task** owns the MQTT `EventLoop`.
//! - The actor is responsible for:
//!   - publishing outbound messages via `AsyncClient`,
//!   - registering broker subscriptions,
//!   - polling the `EventLoop` for incoming publishes,
//!   - clean shutdown of the connection.
//! - All interaction with the MQTT client is serialized through this actor;
//!   no other task ever touches the event loop directly.
//!
//! This design preserves the public `Transport` contract (`Send + Sync`) while
//! respecting the MQTT client's event loop semantics.
//!
//! ## Connection behavior
//!
//! Connection to the broker is **lazy** - it happens when the EventLoop first
//! polls after transport creation. ConnAck success/failure is logged at info/error
//! level respectively. Connection failures use `eprintln!` if logging is disabled
//! to ensure visibility of critical errors.
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
//! ## Subscription confirmation
//!
//! Subscriptions wait for SUBACK confirmation from the broker before returning
//! success. Since rumqttc's `SubAck` packets contain only packet IDs (not topic
//! names), we serialize subscription requests to maintain correlation.
//!
//! ## Scope and limitations
//!
//! - One transport instance corresponds to a single broker connection.
//! - The transport assumes a small number of active topics and subscribers
//!   (typical RPC-style usage).
//! - Text-based payloads (JSON) are decoded in the actor before fanout; payload
//!   parsing cost dominates over synchronization overhead.
//! - Subscriptions are serialized (one at a time) for SUBACK correlation.
//!
//! This module intentionally avoids exposing MQTT-specific concepts (QoS,
//! retain flags, session state) outside the transport boundary.

use rumqttc::{
    //
    AsyncClient,
    ConnectReturnCode,
    Event,
    EventLoop,
    MqttOptions,
    Packet,
    Publish,
    QoS,
};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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

const RECONNECT_DELAY: Duration = Duration::from_secs(2);

//
// Logging macros (transport-local for now; intended to be shared later)
//

macro_rules! log_debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::debug!($($arg)*);
    };
}

macro_rules! log_info {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::info!($($arg)*);
    };
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        #[cfg(feature = "logging")]
        log::error!($($arg)*);
        #[cfg(not(feature = "logging"))]
        eprintln!($($arg)*);
    };
}

type SubscriberMap = Arc<RwLock<HashMap<String, Vec<mpsc::Sender<Envelope>>>>>;
type TaskList = Arc<RwLock<Vec<JoinHandle<()>>>>;

/// Tracks a single pending subscription awaiting SUBACK confirmation.
///
/// Since rumqttc's SubAck packets contain only packet IDs (not topic names),
/// we serialize subscription requests to maintain correlation. This is
/// acceptable for RPC use cases where subscriptions are rare and typically
/// occur at startup.
type PendingSubscribe = Arc<RwLock<Option<(String, oneshot::Sender<Result<()>>)>>>;

//
// Actor commands
//

enum Cmd {
    //
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
    //
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
                let result = actor.handle_publish(env).await;
                let _ = resp.send(result);
                ActorStep::Continue
            }
            Cmd::Subscribe { topic, resp } => {
                actor.handle_subscribe(topic, resp).await;
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
///
/// Connection to the broker happens lazily when the EventLoop begins polling.
pub struct RumqttcTransport {
    // ---
    transport_id: String,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl RumqttcTransport {
    // ---

    /// Creates a new rumqttc transport with the given client and event loop.
    ///
    /// This function is infallible - the actual broker connection happens
    /// lazily when the EventLoop starts polling in the background actor.
    pub fn create(
        transport_id: impl Into<String>,
        client: AsyncClient,
        event_loop: EventLoop,
    ) -> TransportPtr {
        // ---

        let transport_id = transport_id.into();

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let subscribers = Arc::new(RwLock::new(HashMap::new()));
        let tasks = Arc::new(RwLock::new(Vec::new()));
        let pending_subscribe = Arc::new(RwLock::new(None));

        let actor = MqttActor {
            transport_id: transport_id.clone(),
            client,
            event_loop,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
            pending_subscribe,
            reconnect: false,
        };

        let handle = tokio::task::spawn(actor.run());
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
    // ---
    transport_id: String, // for logging only
    client: AsyncClient,
    event_loop: EventLoop,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    pending_subscribe: PendingSubscribe,
    reconnect: bool,
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

                event = self.event_loop.poll() => {
                    match event {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let transport_id = self.transport_id.clone();
                            let subscribers = Arc::clone(&self.subscribers);
                            Self::handle_incoming(transport_id, subscribers, publish).await;
                        }
                        Ok(Event::Incoming(Packet::SubAck(suback))) => {
                            let pending_subscribe = Arc::clone(&self.pending_subscribe);
                            let transport_id = self.transport_id.as_str();
                            Self::handle_suback(pending_subscribe, transport_id, suback).await;
                        }
                        Ok(Event::Incoming(Packet::ConnAck(connack))) => {
                            self.handle_connack(connack); // not async

                            if self.reconnect {
                                let topics: Vec<String> = {
                                    let map = self.subscribers.read().await;
                                    map.keys().cloned().collect()
                                };

                                for topic in topics {
                                    if let Err(err) = self.client.subscribe(&topic, QoS::AtMostOnce).await {
                                        log_error!("{}: resubscribe failed for {topic}: {err}", self.transport_id);
                                    } else {
                                        log_info!("{}: resubscribed to {topic}", self.transport_id);
                                    }
                                }
                            }
                        }
                        Ok(_event) => {
                            // Other events (PingResp, PubAck, etc.) - ignore
                            log_debug!("{}: received mqtt event (ignored):{:?}",
                                       self.transport_id, _event);
                        }
                        Err(err) => {
                            if is_disconnect(&err) {
                                self.reconnect = true;
                                log_error!("{}: broker disconnected: {err}", self.transport_id);
                            } else {
                                log_error!("{}: mqtt error: {err}", self.transport_id);
                            }
                            tokio::time::sleep(RECONNECT_DELAY).await;
                            continue;
                        }
                    }
                }
            }
        }
    }

    /// Publishes an envelope to the broker.
    ///
    /// Serializes the envelope as JSON and publishes with QoS 0 (at most once).
    async fn handle_publish(&mut self, env: Envelope) -> Result<()> {
        // ---

        let topic = env.address.0.as_ref();

        let payload = match serde_json::to_vec(&env) {
            Ok(p) => p,
            Err(_err) => {
                log_error!(
                    "{}: failed to serialize publish payload: {_err}",
                    self.transport_id
                );
                return Err(RpcError::Transport);
            }
        };

        self.client
            .publish(topic, QoS::AtMostOnce, false, payload)
            .await
            .map_err(|_err| {
                log_error!(
                    "{}: publish failed for topic {topic}: {_err}",
                    self.transport_id
                );
                RpcError::Transport
            })
    }

    /// Registers a broker subscription and queues it for SUBACK confirmation.
    ///
    /// The subscription is sent to the broker, and the response channel is stored
    /// in pending_subscribe. When the SUBACK arrives, handle_suback() will complete
    /// the response channel.
    ///
    /// Subscriptions are serialized (one at a time) to maintain correlation with
    /// SUBACK packets, which contain only packet IDs (not topic names).
    async fn handle_subscribe(&mut self, topic: String, resp: oneshot::Sender<Result<()>>) {
        // ---

        // Store the pending subscribe for SUBACK correlation
        {
            let mut pending = self.pending_subscribe.write().await;
            if pending.is_some() {
                log_error!(
                    "{}: attempted concurrent subscribe while one is pending",
                    self.transport_id
                );
                let _ = resp.send(Err(RpcError::Transport));
                return;
            }
            *pending = Some((topic.clone(), resp));
        }

        // Send subscribe request to broker
        if let Err(_err) = self.client.subscribe(&topic, QoS::AtMostOnce).await {
            // Remove from pending on immediate error
            let mut pending = self.pending_subscribe.write().await;
            if let Some((_topic, responder)) = pending.take() {
                log_error!(
                    "{}: failed to send subscribe for topic {topic}: {_err}",
                    self.transport_id
                );
                let _ = responder.send(Err(RpcError::Transport));
            }
        }

        // SUBACK will be handled by handle_suback() when it arrives
    }

    /// Processes SUBACK confirmation from the broker.
    ///
    /// Completes the pending subscription by checking the return codes and
    /// logging the result at info level (since subscriptions are rare events).
    async fn handle_suback(
        pending_subscribe: PendingSubscribe,
        transport_id: &str,
        suback: rumqttc::SubAck,
    ) {
        // ---

        let mut pending = pending_subscribe.write().await;
        let Some((topic, responder)) = pending.take() else {
            // This is a reconnect re-subscribe SUBACK — ignore
            log_debug!("{transport_id}: SUBACK received for reconnect re-subscribe");
            return;
        };

        // Check if subscription was successful
        // Success codes are Success(_) variants, failure is Failure
        let success = suback
            .return_codes
            .iter()
            .all(|code| !matches!(code, rumqttc::SubscribeReasonCode::Failure));

        if success {
            log_info!("{transport_id}: successfully subscribed to topic {topic}");
            let _ = responder.send(Ok(()));
        } else {
            log_error!(
                "{transport_id}: subscription failed for topic {topic}: {:?}",
                suback.return_codes
            );
            let _ = responder.send(Err(RpcError::Transport));
        }
    }

    /// Processes connection acknowledgment from the broker.
    ///
    /// Logs connection success at info level or failure at error level.
    /// Connection failures are always visible (via eprintln if logging disabled)
    /// since they are critical for debugging.
    fn handle_connack(&self, connack: rumqttc::ConnAck) {
        // ---

        if connack.code == ConnectReturnCode::Success {
            log_info!("{}: connected to broker", self.transport_id);
        } else {
            log_error!(
                "{}: connection failed: {:?}",
                self.transport_id,
                connack.code
            );
        }
    }

    /// Disconnects from the MQTT broker.
    async fn handle_close(&mut self) {
        // ---

        log_debug!("{}: disconnecting mqtt client", self.transport_id);

        if let Err(_err) = self.client.disconnect().await {
            log_debug!("{}: mqtt disconnect failed: {_err}", self.transport_id);
        }
    }

    /// Processes incoming MQTT publish messages and fans them out to local subscribers.
    ///
    /// Deserializes the envelope, looks up matching subscribers, and delivers the
    /// message to all live subscribers. Dead or slow subscribers are automatically
    /// evicted during delivery.
    async fn handle_incoming(_transport_id: String, subscribers: SubscriberMap, publish: Publish) {
        // ---

        let topic = publish.topic.clone();
        let payload = publish.payload.clone();

        let env = match serde_json::from_slice::<Envelope>(&payload) {
            Ok(env) => env,
            Err(_err) => {
                log_debug!(
                    "{}: invalid envelope on topic {topic}: {_err}",
                    _transport_id
                );
                return;
            }
        };

        let senders = {
            let map = subscribers.read().await;
            map.get(&topic).cloned()
        };

        let Some(senders) = senders else {
            // No subscribers for this topic
            return;
        };

        // Snapshot the original subscriber count before consuming `senders`.
        let original_len = senders.len();

        // Collect only the surviving (live) subscribers.
        let mut survivors = Vec::with_capacity(original_len);

        for tx in senders {
            match tx.try_send(env.clone()) {
                Ok(()) => {
                    // Subscriber is alive and accepted the message.
                    survivors.push(tx);
                }
                Err(_) => {
                    // Channel is full or receiver was dropped; evict.
                }
            }
        }

        // Only update the map if something changed.
        if survivors.len() != original_len {
            let mut map = subscribers.write().await;
            map.insert(topic, survivors);
        }
    }
} // MqttActor

fn is_disconnect(err: &rumqttc::ConnectionError) -> bool {
    // ---
    matches!(
        err,
        rumqttc::ConnectionError::Io(_) | rumqttc::ConnectionError::MqttState(_)
    )
}

#[async_trait::async_trait]
impl Transport for RumqttcTransport {
    // ---

    fn transport_id(&self) -> &str {
        // ---
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

/// Creates a rumqttc-based MQTT transport from the given configuration.
///
/// # Errors
///
/// Returns an error if:
/// - The broker URL cannot be parsed
/// - URL parsing fails (invalid format)
///
/// # Connection Behavior
///
/// The actual connection to the broker happens lazily when the EventLoop
/// starts polling in the background actor task.
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---

    let (client, event_loop) = create_mqtt_client(config)?;
    Ok(RumqttcTransport::create("rumqttc", client, event_loop))
}

/// Creates an MQTT client and event loop from the given configuration.
///
/// This function is fallible only due to URL parsing. The `AsyncClient::new()`
/// call itself is infallible - connection happens lazily on first poll.
fn create_mqtt_client(config: &RpcConfig) -> Result<(AsyncClient, EventLoop)> {
    // ---

    let broker_addr = &config.broker_addr;
    let client_id = &config.transport_id;

    // Parse broker address (e.g., "mqtt://localhost:1883")
    let url = broker_addr
        .strip_prefix("mqtt://")
        .or_else(|| broker_addr.strip_prefix("tcp://"))
        .unwrap_or(broker_addr);

    let (host, port) = match url.split_once(':') {
        Some((h, p)) => (
            h,
            p.parse().map_err(|_err| {
                log_error!(
                    "rumqttc: invalid port in broker URL {}: {_err}",
                    broker_addr
                );
                RpcError::Transport
            })?,
        ),
        None => (url, 1883),
    };

    let mut mqtt_options = MqttOptions::new(client_id, host, port);

    if let Some(keep_alive_secs) = config.keep_alive_secs {
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(keep_alive_secs as u64));
    }

    let (client, event_loop) = AsyncClient::new(mqtt_options, 10);

    Ok((client, event_loop))
}
