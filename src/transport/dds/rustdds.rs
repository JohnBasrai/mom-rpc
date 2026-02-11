//! DDS transport implementation using `rustdds`.
//!
//! This module provides an implementation of the `Transport` trait backed by DDS
//! (Data Distribution Service) using the RTPS wire protocol. It follows an
//! **actor-based concurrency model** to safely integrate with the underlying DDS
//! library.
//!
//! ## Concurrency model
//!
//! - A single background **actor task** owns the DDS `DataWriter` and `DataReader` instances.
//! - The actor is responsible for:
//!   - publishing outbound messages via `DataWriter`,
//!   - registering topic subscriptions,
//!   - polling all `DataReader` async streams for incoming samples,
//!   - clean shutdown of the `DomainParticipant`.
//! - All interaction with the DDS entities is serialized through this actor;
//!   no other task ever touches them directly.
//!
//! This design preserves the public `Transport` contract (`Send + Sync`) while
//! respecting the DDS library's ownership semantics.
//!
//! ## Connection behavior
//!
//! DDS discovery is **automatic and brokerless** - peers discover each other via
//! RTPS multicast. The DomainParticipant joins the domain at creation time.
//!
//! ## Message delivery semantics
//!
//! Incoming DDS samples are **demultiplexed by topic** and **fanned out**
//! to all local subscribers registered for that topic, matching the memory
//! transport contract:
//!
//! - Fanout delivers messages to *all* subscribers.
//! - Delivery is reliable with QoS::Reliable (consistent with MQTT/AMQP transports).
//! - No replay or persistence (Volatile durability, KeepLast(1) history).
//!
//! Each call to `subscribe()` registers a new local inbox channel. Multiple
//! subscribers for the same topic are supported.
//!
//! ## QoS Configuration
//!
//! The transport uses these QoS policies for RPC semantics:
//! - `Reliability::Reliable` - TCP-like delivery with retries (consistent with other transports)
//! - `History::KeepLast(1)`  - Only latest message (prevents correlation confusion)
//! - `Durability::Volatile`  - No persistence (ephemeral, point-to-point)
//!
//! ## Scope and limitations
//!
//! - One transport instance corresponds to a single `DomainParticipant`.
//! - The transport assumes a small number of active topics and subscribers
//!   (typical RPC-style usage).
//! - Subscriptions create new `DataReader` instances per topic.
//! - Domain ID parsed from transport_uri format: `dds:45` (domain 45)

use rustdds::{
    //
    no_key::{DataReader, DataWriter},
    policy,
    serialization::CDRDeserializerAdapter,
    serialization::CDRSerializerAdapter,
    DomainParticipant,
    QosPolicyBuilder,
    TopicKind,
};

use futures::StreamExt; // For async stream operations
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;

use crate::{
    //
    Envelope,
    Result,
    RpcConfig,
    RpcError,
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

//
// Actor commands
//

enum Cmd {
    //
    Publish {
        topic: String,
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
    async fn handle(self, actor: &mut DdsActor) -> ActorStep {
        // ---

        match self {
            Cmd::Publish { topic, env, resp } => {
                let result = actor.handle_publish(topic, env).await;
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

/// DDS-based implementation of the `Transport` trait.
///
/// Represents a single DomainParticipant and provides reliable,
/// non-durable message delivery consistent with other transports' semantics.
pub struct RustddsTransport {
    // ---
    transport_id: String,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl RustddsTransport {
    // ---

    /// Creates a new rustdds transport with the given `DomainParticipant`.
    pub fn create(transport_id: impl Into<String>, participant: DomainParticipant) -> TransportPtr {
        // ---

        let transport_id = transport_id.into();

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let subscribers = Arc::new(RwLock::new(HashMap::new()));
        let tasks = Arc::new(RwLock::new(Vec::new()));

        let actor = DdsActor {
            transport_id: transport_id.clone(),
            participant,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
            writers: HashMap::new(),
            reader_tasks: Vec::new(),
            has_sent_first_message: HashMap::new(),
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

struct DdsActor {
    // ---
    transport_id: String, // for logging only
    participant: DomainParticipant,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    writers: HashMap<String, DataWriter<Envelope, CDRSerializerAdapter<Envelope>>>,
    reader_tasks: Vec<JoinHandle<()>>,
    has_sent_first_message: HashMap<String, bool>, // Track first publish per topic for discovery verification
}

impl DdsActor {
    // ---

    async fn run(mut self) {
        // ---

        log_info!("{}: DDS actor started", self.transport_id);

        while let Some(cmd) = self.cmd_rx.recv().await {
            if matches!(cmd.handle(&mut self).await, ActorStep::Stop) {
                break;
            }
        }

        // Cleanup reader tasks
        for handle in self.reader_tasks {
            handle.abort();
        }

        log_info!("{}: DDS actor stopped", self.transport_id);
    }

    async fn handle_publish(&mut self, topic: String, env: Envelope) -> Result<()> {
        // ---
        log_debug!(
            "{}: handle_publish() called for topic {topic}",
            self.transport_id,
        );

        // Get or create DataWriter for this topic
        if !self.writers.contains_key(&topic) {
            log_debug!(
                "{}: creating new DataWriter for topic {topic}",
                self.transport_id,
            );
            self.create_writer(&topic)?;
        }

        let writer = self.writers.get_mut(&topic).ok_or_else(|| {
            RpcError::Transport(format!("DataWriter not found for topic {topic}"))
        })?;

        writer.write(env, None).map_err(|e| {
            let msg = format!(
                "{}: write failed for topic {topic}: {e:?}",
                self.transport_id
            );
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?;

        // Wait for acknowledgment ONLY on first publish to verify discovery
        // Subsequent publishes are fire-and-forget for better throughput
        if !self.has_sent_first_message.get(&topic).unwrap_or(&false) {
            // ---

            log_debug!(
                "{}: waiting for first ack on topic {topic} to verify discovery",
                self.transport_id
            );

            let ack_timeout = tokio::time::Duration::from_secs(5);
            match tokio::time::timeout(ack_timeout, writer.async_wait_for_acknowledgments()).await {
                Ok(Ok(_)) => {
                    self.has_sent_first_message.insert(topic.clone(), true);
                    log_debug!(
                        "{}: first publish confirmed for topic {topic}, future publishes will be fire-and-forget",
                        self.transport_id
                    );
                }
                Ok(Err(e)) => {
                    return Err(RpcError::Transport(format!(
                        "{}: acknowledgment failed for topic {topic}: {e:?}",
                        self.transport_id
                    )));
                }
                Err(_timeout) => {
                    return Err(RpcError::Transport(format!(
                        "{}: timeout waiting for acknowledgment on topic {topic} - no matched readers?",
                        self.transport_id
                    )));
                }
            }
        }

        log_debug!("{}: published to topic {topic}", self.transport_id);
        Ok(())
    }

    async fn handle_subscribe(&mut self, topic: String, resp: oneshot::Sender<Result<()>>) {
        // ---

        // Create DataReader and spawn polling task
        let result = self.create_reader_and_poll(&topic);

        match result {
            Ok(()) => {
                log_info!("{}: subscribed to topic {}", self.transport_id, topic);
                let _ = resp.send(Ok(()));
            }
            Err(e) => {
                log_error!(
                    "{}: subscription failed for topic {}: {:?}",
                    self.transport_id,
                    topic,
                    e
                );
                let _ = resp.send(Err(e));
            }
        }
    }

    fn create_writer(&mut self, topic: &str) -> Result<()> {
        // ---

        let qos = build_rpc_qos();

        let topic_obj = self
            .participant
            .create_topic(
                topic.to_string(),
                "Envelope".to_string(),
                &qos,
                TopicKind::NoKey,
            )
            .map_err(|e| RpcError::Transport(format!("create_topic failed: {e:?}")))?;

        let publisher = self
            .participant
            .create_publisher(&qos)
            .map_err(|e| RpcError::Transport(format!("create_publisher failed: {e:?}")))?;

        let writer = publisher
            .create_datawriter_no_key(&topic_obj, None)
            .map_err(|e| RpcError::Transport(format!("create_datawriter failed: {e:?}")))?;

        self.writers.insert(topic.to_string(), writer);
        log_debug!(
            "{}: created DataWriter for topic {}",
            self.transport_id,
            topic
        );
        Ok(())
    }

    fn create_reader_and_poll(&mut self, topic: &str) -> Result<()> {
        // ---

        let qos = build_rpc_qos();

        let topic_obj = self
            .participant
            .create_topic(
                topic.to_string(),
                "Envelope".to_string(),
                &qos,
                TopicKind::NoKey,
            )
            .map_err(|e| RpcError::Transport(format!("create_topic failed: {e:?}")))?;

        let subscriber = self
            .participant
            .create_subscriber(&qos)
            .map_err(|e| RpcError::Transport(format!("create_subscriber failed: {e:?}")))?;

        let reader = subscriber
            .create_datareader_no_key::<Envelope, CDRDeserializerAdapter<Envelope>>(
                &topic_obj, None,
            )
            .map_err(|e| RpcError::Transport(format!("create_datareader failed: {e:?}")))?;

        log_debug!(
            "{}: created DataReader for topic {}",
            self.transport_id,
            topic
        );

        // Spawn task to poll this reader's async stream
        let transport_id = self.transport_id.clone();
        let topic_name = topic.to_string();
        let subscribers = Arc::clone(&self.subscribers);

        let handle = tokio::spawn(async move {
            poll_reader_stream(transport_id, topic_name, reader, subscribers).await;
        });

        self.reader_tasks.push(handle);
        Ok(())
    }

    async fn handle_close(&mut self) {
        // ---

        log_debug!("{}: closing DDS transport", self.transport_id);
        // DomainParticipant will be dropped, cleaning up all entities
    }
}

/// Polls a `DataReader`'s async stream and fans out incoming samples to local subscribers.
async fn poll_reader_stream(
    transport_id: String,
    topic: String,
    reader: DataReader<Envelope, CDRDeserializerAdapter<Envelope>>,
    subscribers: SubscriberMap,
) {
    // ---

    log_debug!(
        "{}: polling DataReader stream for topic {}",
        transport_id,
        topic
    );

    // Get async stream from DataReader
    let mut stream = reader.async_bare_sample_stream();

    while let Some(sample_result) = stream.next().await {
        match sample_result {
            Ok(env) => {
                handle_incoming(&transport_id, &topic, Arc::clone(&subscribers), env).await;
            }
            Err(e) => {
                log_debug!("{transport_id}: sample read error on topic {topic}: {e:?}");
            }
        }
    }

    log_debug!(
        "{}: DataReader stream ended for topic {}",
        transport_id,
        topic
    );
}

/// Fans out an incoming envelope to all local subscribers for the topic.
async fn handle_incoming(
    transport_id: &str,
    topic: &str,
    subscribers: SubscriberMap,
    env: Envelope,
) {
    // ---

    let senders = {
        let map = subscribers.read().await;
        map.get(topic).cloned()
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
                log_debug!(
                    "{}: evicted dead subscriber for topic {}",
                    transport_id,
                    topic
                );
            }
        }
    }

    // Only update the map if something changed.
    if survivors.len() != original_len {
        let mut map = subscribers.write().await;
        map.insert(topic.to_string(), survivors);
    }
}

/// Builds QoS policies appropriate for RPC semantics.
///
/// - `Reliability::Reliable` - TCP-like delivery with retries (consistent with MQTT/AMQP)
/// - `History::KeepLast(1)` - Only latest message (prevents correlation confusion)
/// - `Durability::Volatile` - No persistence (ephemeral, point-to-point)
fn build_rpc_qos() -> rustdds::QosPolicies {
    // ---

    QosPolicyBuilder::new()
        .reliability(policy::Reliability::Reliable {
            max_blocking_time: rustdds::Duration::ZERO, // Non-blocking writes
        })
        .history(policy::History::KeepLast { depth: 1 })
        .durability(policy::Durability::Volatile)
        .build()
}

#[async_trait::async_trait]
impl Transport for RustddsTransport {
    // ---

    fn transport_id(&self) -> &str {
        // ---
        &self.transport_id
    }

    async fn publish(&self, env: Envelope) -> Result<()> {
        // ---
        let topic = env.address.0.to_string();
        log_debug!(
            "{}: publish() called for topic {}",
            self.transport_id,
            topic
        );

        let (tx, rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Publish {
                topic,
                env,
                resp: tx,
            })
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
        log_debug!(
            "{}: subscribe() called for topic {}",
            self.transport_id,
            topic
        );

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

/// Creates a rustdds-based DDS transport from the given configuration.
///
/// # Configuration
///
/// - `transport_uri`: Expected format is `dds:<domain_id>` (e.g., `dds:0`, `dds:45`)
///   If not provided or parsing fails, defaults to domain 0.
///
/// # Errors
///
/// Returns an error if `DomainParticipant` creation fails.
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---

    // Parse domain ID from transport_uri (format: dds:45)
    let domain_id = parse_domain_id(config.transport_uri.as_deref());

    let participant = DomainParticipant::new(domain_id).map_err(|e| {
        let msg = format!("Failed to create DomainParticipant on domain {domain_id}: {e:?}");
        log_error!("{msg}");
        RpcError::Transport(msg)
    })?;

    log_info!(
        "{}: created DomainParticipant on domain {}",
        config.transport_id,
        domain_id
    );

    Ok(RustddsTransport::create(&config.transport_id, participant))
}

/// Parses domain ID from `transport_uri`.
///
/// Expected format: `dds:<domain_id>` (e.g., "dds:0", "dds:45")
/// Returns 0 (default domain) if uri is `None` or parsing fails.
fn parse_domain_id(uri: Option<&str>) -> u16 {
    // ---

    let Some(uri) = uri else {
        log_debug!("No transport_uri provided, using default DDS domain 0");
        return 0;
    };

    let domain_str = uri.strip_prefix("dds:").unwrap_or_else(|| {
        log_debug!("transport_uri does not start with 'dds:', using default domain 0");
        ""
    });

    domain_str.parse().unwrap_or_else(|_| {
        log_debug!(
            "Failed to parse domain ID from '{}', using default domain 0",
            uri
        );
        0
    })
}
