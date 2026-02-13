//! AMQP transport implementation using `lapin`.
//!
//! This module provides an implementation of the `Transport` trait backed by
//! an AMQP broker connection. It follows an **actor-based concurrency model**
//! to safely integrate with the underlying AMQP client.
//!
//! ## Concurrency model
//!
//! - A single background **actor task** owns the AMQP connection and channel.
//! - The actor is responsible for:
//!   - publishing outbound messages,
//!   - declaring and binding queues,
//!   - consuming incoming messages,
//!   - clean shutdown of the connection.
//! - All interaction with the AMQP client is serialized through this actor;
//!   no other task ever touches the connection directly.
//!
//! This design preserves the public `Transport` contract (`Send + Sync`) while
//! respecting the AMQP client's connection semantics.
//!
//! ## Connection behavior
//!
//! Connection to the broker is **lazy** - it happens when the actor task starts.
//! Connection success/failure is logged at info/error level respectively.
//! Connection failures use `eprintln!` if logging is disabled to ensure
//! visibility of critical errors.
//!
//! ## Message delivery semantics
//!
//! Incoming AMQP messages are **demultiplexed by queue name** and **fanned out**
//! to all local subscribers registered for that queue, matching the memory
//! transport contract:
//!
//! - Fanout delivers messages to *all* subscribers.
//! - Delivery is best-effort and non-durable.
//! - Queues are ephemeral (auto-delete, non-durable, non-exclusive).
//!
//! Each call to `subscribe()` registers a new local inbox channel. Multiple
//! subscribers for the same queue are supported.
//!
//! ## Queue semantics
//!
//! Queues are declared with hardcoded options suitable for RPC:
//! - `durable: false` - Messages not persisted to disk
//! - `auto_delete: true` - Queue deleted when last consumer disconnects
//! - `exclusive: false` - Multiple consumers allowed
//!
//! Queue names are derived from `transport_id` unless custom names are
//! provided via `RpcConfig::request_queue_name` or `response_queue_name`.
//!
//! ## Scope and limitations
//!
//! - One transport instance corresponds to a single broker connection.
//! - The transport assumes a small number of active queues and subscribers
//!   (typical RPC-style usage).
//! - Messages are JSON-encoded and decoded in the actor before fanout.
//!
//! This module intentionally avoids exposing AMQP-specific concepts
//! (exchanges, routing keys, message properties) outside the transport boundary.

use lapin::{
    //
    options::{
        //
        BasicAckOptions,
        BasicConsumeOptions,
        BasicPublishOptions,
        QueueDeclareOptions,
    },
    types::FieldTable,
    BasicProperties,
    Channel,
    Connection,
    ConnectionProperties,
};

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;

use crate::{
    //
    log_debug,
    log_error,
    log_info,
    Envelope,
    Result,
    RpcConfig,
    RpcError,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};

type SubscriberMap = Arc<RwLock<HashMap<String, Vec<mpsc::Sender<Envelope>>>>>;
type TaskList = Arc<RwLock<Vec<JoinHandle<()>>>>;

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
        queue: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Close {
        resp: oneshot::Sender<Result<()>>,
    },
}

enum ActorStep {
    //
    Cmd(Cmd),
    Closed,
}

/// AMQP transport implementation using lapin.
///
/// This struct is cheap to clone (Arc-based internally) and implements
/// `Send + Sync` for use across async boundaries.
pub struct AmqpTransport {
    // ---
    transport_id: String,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl AmqpTransport {
    /// Creates a new AMQP transport with the given connection and channel.
    ///
    /// Spawns a background actor task to handle AMQP operations.
    fn create(transport_id: &str, connection: Connection, channel: Channel) -> TransportPtr {
        // ---
        let transport_id = transport_id.to_string();

        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let subscribers: SubscriberMap = Arc::new(RwLock::new(HashMap::new()));
        let tasks: TaskList = Arc::new(RwLock::new(Vec::new()));

        let actor = Actor {
            transport_id: transport_id.clone(),
            connection,
            channel,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
            consumer_handles: HashMap::new(),
        };

        let handle = tokio::spawn(async move {
            actor.run().await;
        });

        {
            let tasks_clone = Arc::clone(&tasks);
            tokio::spawn(async move {
                tasks_clone.write().await.push(handle);
            });
        }

        Arc::new(Self {
            transport_id,
            cmd_tx,
            subscribers,
            tasks,
        })
    }
}

/// Background actor task that owns the AMQP connection and channel.
struct Actor {
    // ---
    transport_id: String,
    connection: Connection,
    channel: Channel,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    consumer_handles: HashMap<String, JoinHandle<()>>,
}

impl Actor {
    async fn run(mut self) {
        // ---
        log_info!("[{}] AMQP actor started", self.transport_id);

        loop {
            match self.next_step().await {
                ActorStep::Cmd(cmd) => {
                    self.handle_cmd(cmd).await;
                }
                ActorStep::Closed => {
                    log_info!("[{}] AMQP actor shutting down", self.transport_id);
                    break;
                }
            }
        }

        // Clean up consumer tasks
        for (_, handle) in self.consumer_handles.drain() {
            handle.abort();
        }

        // Close channel and connection
        let _ = self.channel.close(200, "Normal shutdown".into()).await;
        let _ = self.connection.close(200, "Normal shutdown".into()).await;

        log_info!("[{}] AMQP actor stopped", self.transport_id);
    }

    async fn next_step(&mut self) -> ActorStep {
        // ---
        match self.cmd_rx.recv().await {
            Some(cmd) => ActorStep::Cmd(cmd),
            None => ActorStep::Closed,
        }
    }

    async fn handle_cmd(&mut self, cmd: Cmd) {
        // ---
        match cmd {
            Cmd::Publish { env, resp } => {
                let result = self.do_publish(env).await;
                let _ = resp.send(result);
            }
            Cmd::Subscribe { queue, resp } => {
                let result = self.do_subscribe(queue).await;
                let _ = resp.send(result);
            }
            Cmd::Close { resp } => {
                let _ = resp.send(Ok(()));
                self.cmd_rx.close();
            }
        }
    }

    async fn do_publish(&mut self, env: Envelope) -> Result<()> {
        // ---
        let queue = env.address.0.as_ref();
        let payload = serde_json::to_vec(&env)
            .map_err(|e| RpcError::Transport(format!("amqp: failed to serialize envelope: {e}")))?;

        self.channel
            .basic_publish(
                "".into(),    // default exchange
                queue.into(), // routing key = queue name
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| RpcError::Transport(format!("amqp: publish failed: {e}")))?;

        log_debug!("[{}] Published to queue: {queue}", self.transport_id);
        Ok(())
    }

    async fn do_subscribe(&mut self, queue: String) -> Result<()> {
        // ---

        // Declare queue if not already declared
        let queue_opts = QueueDeclareOptions {
            passive: false,
            durable: false,
            exclusive: false,
            auto_delete: true,
            nowait: false,
        };

        self.channel
            .queue_declare(queue.clone().into(), queue_opts, FieldTable::default())
            .await
            .map_err(|e| RpcError::Transport(format!("amqp: queue declare failed: {e}")))?;

        log_info!("[{}] Declared queue: {queue}", self.transport_id);

        // Start consumer if not already consuming this queue
        if self.consumer_handles.contains_key(&queue) {
            log_debug!("[{}] Already consuming queue: {queue}", self.transport_id);
            return Ok(());
        }

        let consumer = self
            .channel
            .basic_consume(
                queue.clone().into(),
                format!("{}-consumer", self.transport_id).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| RpcError::Transport(format!("amqp: consume failed: {e}")))?;

        log_info!("[{}] Started consuming queue: {queue}", self.transport_id);

        // Spawn consumer task
        let queue_clone = queue.clone();
        let transport_id = self.transport_id.clone();
        let subscribers = Arc::clone(&self.subscribers);

        let handle = tokio::spawn(async move {
            use futures_lite::stream::StreamExt;

            let mut consumer = consumer;
            while let Some(delivery_result) = consumer.next().await {
                match delivery_result {
                    Ok(delivery) => {
                        log_debug!("[{transport_id}] Received message on queue: {queue_clone}");

                        // Ack the message
                        if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                            log_error!("[{transport_id}] Failed to ack message: {e}");
                            continue;
                        }

                        // Deserialize envelope
                        let envelope: Envelope = match serde_json::from_slice(&delivery.data) {
                            Ok(env) => env,
                            Err(e) => {
                                log_error!("[{transport_id}] Failed to deserialize envelope: {e}");
                                continue;
                            }
                        };

                        // Fanout to local subscribers
                        let subs = subscribers.read().await;
                        if let Some(senders) = subs.get(&queue_clone) {
                            for sender in senders {
                                if let Err(e) = sender.send(envelope.clone()).await {
                                    log_error!(
                                        "[{transport_id}] Failed to send to subscriber: {e}"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log_error!("[{transport_id}] Consumer error on {queue_clone}: {e}");
                        break;
                    }
                }
            }

            log_info!("[{transport_id}] Consumer task ended for queue: {queue_clone}");
        });

        self.consumer_handles.insert(queue.clone(), handle);

        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for AmqpTransport {
    // ---
    fn transport_id(&self) -> &str {
        &self.transport_id
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

        let queue = sub.0.as_ref().to_string();

        let (tx, rx) = mpsc::channel(16);
        {
            let mut map = self.subscribers.write().await;
            map.entry(queue.clone()).or_default().push(tx);
        }

        let (resp_tx, resp_rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Subscribe {
                queue,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                let msg = format!("actor command channel closed:{e}");
                RpcError::Transport(msg)
            })?;

        resp_rx.await.map_err(|e| {
            let msg = format!("actor resp_rx channel read failed:{e}");
            RpcError::Transport(msg)
        })??;

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

/// Creates a lapin-based AMQP transport from the given configuration.
///
/// # Errors
///
/// Returns an error if:
/// - The broker URI is missing or cannot be parsed
/// - Connection to the broker fails
///
/// # Connection Behavior
///
/// The connection to the broker happens immediately during transport creation.
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---

    let (connection, channel) = create_amqp_connection(config).await?;
    Ok(AmqpTransport::create(
        &config.transport_id,
        connection,
        channel,
    ))
}

/// Creates an AMQP connection and channel from the given configuration.
async fn create_amqp_connection(config: &RpcConfig) -> Result<(Connection, Channel)> {
    // ---

    let uri = config
        .transport_uri
        .as_deref()
        .ok_or_else(|| RpcError::Transport("AMQP transport requires transport_uri".to_string()))?;

    log_info!("Connecting to AMQP broker: {uri}");

    let connection = Connection::connect(uri, ConnectionProperties::default())
        .await
        .map_err(|e| {
            let msg = format!("amqp: connection failed: {e}");
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?;

    log_info!("Connected to AMQP broker");

    let channel = connection.create_channel().await.map_err(|e| {
        let msg = format!("amqp: channel creation failed: {e}");
        log_error!("{msg}");
        RpcError::Transport(msg)
    })?;

    log_info!("Created AMQP channel");

    Ok((connection, channel))
}
