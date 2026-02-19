//! Redis Pub/Sub transport implementation using `redis`.
//!
//! This module provides an implementation of the `Transport` trait backed by
//! a Redis Pub/Sub connection. It follows an **actor-based concurrency model**
//! identical to the rumqttc transport.
//!
//! ## Concurrency model
//!
//! - A single background **actor task** owns both Redis connections.
//! - The actor is responsible for:
//!   - publishing outbound messages via `publish_conn`,
//!   - registering broker subscriptions via `pubsub_sink`,
//!   - polling `pubsub_stream` for incoming data messages,
//!   - clean shutdown of both connections.
//! - All interaction with the Redis client is serialized through this actor;
//!   no other task ever touches the connections directly.
//!
//! ## Two connections required
//!
//! Redis mandates a dedicated connection for Pub/Sub — a connection in
//! Pub/Sub mode cannot issue regular commands like `PUBLISH`. Two async
//! connections are therefore maintained:
//!
//! - `publish_conn` — `MultiplexedConnection`, used only for `PUBLISH`
//! - `pubsub_sink` / `pubsub_stream` — split from `aio::PubSub`, used for
//!   `SUBSCRIBE` and receiving incoming messages respectively
//!
//! `split()` is used so that `pubsub_sink` can call `subscribe()` concurrently
//! with `pubsub_stream` being polled in `select!`.
//!
//! ## Subscription confirmation
//!
//! In redis 1.0.3, `PubSubSink::subscribe()` is async and resolves only after
//! the broker confirms the subscription — there is no separate confirmation
//! message on `PubSubStream`. The stream yields only data `Msg` values.
//!
//! The `Ack::Retry` serialization from rumqttc is preserved to guard against
//! concurrent subscription races: if a subscribe is already in flight, the
//! actor responds `Ack::Retry` and the caller backs off 200 ms. Because
//! `sink.subscribe().await` itself blocks until confirmed, we respond
//! `Ack::Ok` immediately after it returns — there is no separate
//! `handle_subscribe_confirm` step.
//!
//! ## Message delivery semantics
//!
//! Incoming Redis publishes are demultiplexed by channel name and fanned out
//! to all local subscribers registered for that topic, matching the memory
//! and MQTT transport contracts:
//!
//! - Fanout delivers messages to *all* subscribers.
//! - Delivery is best-effort and non-durable.
//! - There is no replay, persistence, or retained-message support.

use futures_util::StreamExt;

use redis::aio::{MultiplexedConnection, PubSubSink, PubSubStream};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Notify, RwLock};
use tokio::task::JoinHandle;

use crate::{
    //
    log_debug,
    log_error,
    log_info,
    Envelope,
    Result,
    RpcError,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportBase,
    TransportConfig,
    TransportPtr,
};

const RECONNECT_DELAY: Duration = Duration::from_secs(2);

type SubscriberMap = Arc<RwLock<HashMap<String, Vec<mpsc::Sender<Envelope>>>>>;
type TaskList = Arc<RwLock<Vec<JoinHandle<()>>>>;

/// Tracks a single pending subscription awaiting sink confirmation.
///
/// `PubSubSink::subscribe()` is async but the actor cannot service `cmd_rx`
/// while awaiting it. We therefore allow only one subscribe in flight at a
/// time: if a second arrives, the actor responds `Ack::Retry` and the caller
/// backs off 200 ms before retrying. This matches the SUBACK serialization
/// in the rumqttc transport.
type PendingSubscribe = Arc<RwLock<bool>>;

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
        resp: oneshot::Sender<Result<Ack>>,
    },
    Close {
        resp: oneshot::Sender<Result<()>>,
    },
}

/// Actor Ack
#[derive(PartialEq)]
pub enum Ack {
    Ok,
    Retry,
}

enum ActorStep {
    //
    Continue,
    Stop,
}

impl Cmd {
    // ---

    /// Dispatches an actor command to the correct handler on the actor.
    async fn handle(self, actor: &mut RedisActor) -> ActorStep {
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

/// Redis Pub/Sub implementation of the `Transport` trait.
///
/// Represents a single broker connection pair and provides best-effort,
/// non-durable message delivery consistent with memory and MQTT transport
/// semantics.
pub struct RedisTransport {
    // ---
    base: TransportBase,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl RedisTransport {
    // ---

    /// Creates a new Redis transport from the given connections.
    ///
    /// This function is infallible — the actual broker connections have already
    /// been established in `create_transport` before this is called.
    pub fn create(
        base: TransportBase,
        publish_conn: MultiplexedConnection,
        pubsub_sink: PubSubSink,
        pubsub_stream: PubSubStream,
        shutdown: Arc<Notify>,
    ) -> TransportPtr {
        // ---

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let subscribers = Arc::new(RwLock::new(HashMap::new()));
        let tasks = Arc::new(RwLock::new(Vec::new()));
        let subscribe_pending = Arc::new(RwLock::new(false));

        let actor = RedisActor {
            transport_id: base.transport_id.clone(),
            publish_conn,
            pubsub_sink,
            pubsub_stream,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
            subscribe_pending,
            shutdown,
            reconnect: false,
        };

        let handle = tokio::task::spawn(actor.run());
        let tasks_clone = Arc::clone(&tasks);

        tokio::spawn(async move {
            tasks_clone.write().await.push(handle);
        });

        Arc::new(Self {
            base,
            cmd_tx,
            subscribers,
            tasks,
        })
    }
}

struct RedisActor {
    // ---
    transport_id: String, // for logging only
    publish_conn: MultiplexedConnection,
    pubsub_sink: PubSubSink,
    pubsub_stream: PubSubStream,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    subscribe_pending: PendingSubscribe,
    shutdown: Arc<Notify>,
    reconnect: bool,
}

impl RedisActor {
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

                maybe_msg = self.pubsub_stream.next() => {
                    match maybe_msg {
                        Some(msg) => {
                            let transport_id = self.transport_id.clone();
                            let subscribers = Arc::clone(&self.subscribers);
                            Self::handle_incoming(transport_id, subscribers, msg).await;
                        }
                        None => {
                            // pubsub_stream terminated (connection lost)
                            log_error!("{}: pubsub stream ended", self.transport_id);
                            self.reconnect = true;
                            tokio::time::sleep(RECONNECT_DELAY).await;
                        }
                    }
                }

                _ = self.shutdown.notified() => {
                    break;
                }
            }
        }
    }

    /// Publishes an envelope to the broker.
    ///
    /// Serializes the envelope as JSON and issues a Redis PUBLISH command
    /// on the dedicated multiplexed publish connection.
    async fn handle_publish(&mut self, env: Envelope) -> Result<()> {
        // ---

        let topic = env.address.0.as_ref().to_string();

        let payload = match serde_json::to_string(&env) {
            Ok(p) => p,
            Err(err) => {
                let msg = format!(
                    "{}: failed to serialize publish payload: {err}",
                    self.transport_id
                );
                log_error!("{msg}");
                return Err(RpcError::Transport(msg));
            }
        };

        redis::cmd("PUBLISH")
            .arg(&topic)
            .arg(&payload)
            .query_async::<i64>(&mut self.publish_conn)
            .await
            .map(|_| ())
            .map_err(|err| {
                let msg = format!(
                    "{}: publish failed for topic {topic}: {err}",
                    self.transport_id
                );
                log_error!("{msg}");
                RpcError::Transport(msg)
            })
    }

    /// Registers a broker subscription and awaits sink confirmation.
    ///
    /// In redis 1.0.3, `PubSubSink::subscribe()` is async and returns only
    /// after the broker confirms the subscription — there is no separate
    /// confirmation message on `PubSubStream`. We respond `Ack::Ok` directly
    /// after the call returns.
    ///
    /// Subscriptions are serialized (one at a time) via `subscribe_pending`
    /// to prevent the actor from blocking `cmd_rx` with concurrent awaits.
    async fn handle_subscribe(&mut self, topic: String, resp: oneshot::Sender<Result<Ack>>) {
        // ---

        let transport_id = self.transport_id.as_str();

        {
            let mut pending = self.subscribe_pending.write().await;
            if *pending {
                log_debug!("{transport_id}: handle_subscribe: subscribe already pending, retry...");
                let _ = resp.send(Ok(Ack::Retry));
                return;
            }
            *pending = true;
        }

        let result = self.pubsub_sink.subscribe(&topic).await;

        {
            let mut pending = self.subscribe_pending.write().await;
            *pending = false;
        }

        match result {
            Ok(()) => {
                log_info!("{transport_id}: successfully subscribed to topic {topic}");
                let _ = resp.send(Ok(Ack::Ok));
            }
            Err(err) => {
                let msg = format!("{transport_id}: failed to subscribe to topic {topic}: {err}");
                log_error!("{msg}");
                let _ = resp.send(Err(RpcError::Transport(msg)));
            }
        }
    }

    /// Disconnects from the Redis broker.
    async fn handle_close(&mut self) {
        // ---

        log_debug!("{}: disconnecting redis client", self.transport_id);
        self.shutdown.notify_waiters();
    }

    /// Processes incoming Redis Pub/Sub messages and fans them out to local subscribers.
    ///
    /// Deserializes the envelope, looks up matching subscribers, and delivers
    /// the message to all live subscribers. Dead or slow subscribers are
    /// automatically evicted during delivery.
    async fn handle_incoming(_transport_id: String, subscribers: SubscriberMap, msg: redis::Msg) {
        // ---

        let topic = msg.get_channel_name().to_string();

        let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(_err) => {
                log_debug!(
                    "{}: failed to get payload on topic {topic}: {_err}",
                    _transport_id
                );
                return;
            }
        };

        let env = match serde_json::from_str::<Envelope>(&payload) {
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
            return;
        };

        let original_len = senders.len();
        let mut survivors = Vec::with_capacity(original_len);

        for tx in senders {
            match tx.try_send(env.clone()) {
                Ok(()) => {
                    survivors.push(tx);
                }
                Err(_) => {
                    // Channel is full or receiver was dropped; evict.
                }
            }
        }

        if survivors.len() != original_len {
            let mut map = subscribers.write().await;
            map.insert(topic, survivors);
        }
    }
} // RedisActor

#[async_trait::async_trait]
impl Transport for RedisTransport {
    // ---

    fn base(&self) -> &TransportBase {
        &self.base
    }

    async fn publish(&self, env: Envelope) -> Result<()> {
        // ---

        let (tx, rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Publish { env, resp: tx })
            .await
            .map_err(|e| {
                let msg = format!("actor command channel closed:{e}");
                RpcError::Transport(msg)
            })?;

        rx.await.map_err(|e| {
            let msg = format!("actor responder channel read failed:{e}");
            RpcError::Transport(msg)
        })?
    }

    async fn subscribe(&self, sub: Subscription) -> Result<SubscriptionHandle> {
        // ---

        let topic = sub.0.as_ref().to_string();

        let (tx, rx) = mpsc::channel(16);
        {
            let mut map = self.subscribers.write().await;
            map.entry(topic.clone()).or_default().push(tx);
        }

        loop {
            let (resp_tx, resp_rx) = oneshot::channel();

            self.cmd_tx
                .send(Cmd::Subscribe {
                    topic: topic.clone(),
                    resp: resp_tx,
                })
                .await
                .map_err(|e| {
                    let msg = format!("actor command channel closed:{e}");
                    RpcError::Transport(msg)
                })?;

            match resp_rx.await.map_err(|e| {
                let msg = format!("actor resp_rx channel read failed:{e}");
                RpcError::Transport(msg)
            })? {
                Ok(Ack::Retry) => {
                    let delay_ms = 200;
                    log_debug!(
                        "{}: subscribe: Got Ack::Retry, retrying in {delay_ms}ms...",
                        self.base.transport_id
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                Ok(Ack::Ok) => break,
                Err(e) => return Err(e),
            }
        }

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

/// Creates a Redis Pub/Sub transport from the given configuration.
///
/// # Errors
///
/// Returns an error if:
/// - The broker URI is missing or cannot be parsed
/// - Connection to the Redis broker fails (both connections are eager)
pub async fn create_transport(config: TransportConfig) -> Result<TransportPtr> {
    // ---

    let uri = if config.uri.is_empty() {
        return Err(RpcError::Transport(
            "Redis transport requires URI".to_string(),
        ));
    } else {
        &config.uri
    };

    let client = redis::Client::open(uri.as_str()).map_err(|err| {
        let msg = format!("redis: failed to open client for URI {uri}: {err}");
        log_error!("{msg}");
        RpcError::Transport(msg)
    })?;

    let publish_conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|err| {
            let msg = format!("redis: failed to connect publish connection to {uri}: {err}");
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?;

    let (pubsub_sink, pubsub_stream) = client
        .get_async_pubsub()
        .await
        .map_err(|err| {
            let msg = format!("redis: failed to connect pubsub connection to {uri}: {err}");
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?
        .split();

    log_info!("{}: connected to Redis broker at {uri}", config.node_id);

    let shutdown = Arc::new(Notify::new());

    Ok(RedisTransport::create(
        TransportBase::from(&config),
        publish_conn,
        pubsub_sink,
        pubsub_stream,
        shutdown,
    ))
}
