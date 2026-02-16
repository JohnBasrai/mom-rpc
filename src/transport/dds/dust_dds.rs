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

use dust_dds::{
    //
    dds_async::{
        //
        data_reader::DataReaderAsync,
        data_writer::DataWriterAsync,
        domain_participant::DomainParticipantAsync,
        domain_participant_factory::DomainParticipantFactoryAsync,
        wait_set::{ConditionAsync, WaitSetAsync},
    },
    infrastructure::{
        error::DdsError,
        qos::{DataReaderQos, DataWriterQos, QosKind},
        qos_policy::{
            //
            DurabilityQosPolicy,
            DurabilityQosPolicyKind,
            HistoryQosPolicy,
            HistoryQosPolicyKind,
            ReliabilityQosPolicy,
            ReliabilityQosPolicyKind,
        },
        sample_info::{SampleStateKind, ANY_INSTANCE_STATE, ANY_VIEW_STATE},
        status::StatusKind,
        time::{Duration as DdsDuration, DurationKind},
        type_support::DdsType,
    },
    std_runtime::StdRuntime,
};

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, watch, RwLock};
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

/// Constant type name for all DdsEnvelope topics
const DDS_TYPE_NAME: &str = "DdsEnvelope";

//
// DDS-compatible wrapper for Envelope
//

/// A DDS-compatible wrapper for Envelope that can be published over DDS.
///
/// DDS requires all published types to implement TypeSupport, which means
/// all fields must be DDS-serializable. Since Envelope contains Arc<str> and
/// Bytes which aren't directly DDS-compatible, we serialize the entire envelope
/// to JSON bytes for DDS transmission.
#[derive(Clone, Debug, DdsType, serde::Serialize, serde::Deserialize)]
struct DdsEnvelope {
    /// JSON-serialized Envelope
    topic: String,
    /// JSON-serialized envelope data
    data: Vec<u8>,
}

impl TryFrom<&Envelope> for DdsEnvelope {
    type Error = RpcError;

    fn try_from(env: &Envelope) -> Result<Self> {
        let data = serde_json::to_vec(env)
            .map_err(|e| RpcError::Transport(format!("DDS envelope serialization failed: {e}")))?;

        Ok(Self {
            topic: env.address.0.as_ref().to_string(),
            data,
        })
    }
}

impl TryFrom<DdsEnvelope> for Envelope {
    type Error = RpcError;

    fn try_from(dds_env: DdsEnvelope) -> Result<Self> {
        serde_json::from_slice(&dds_env.data)
            .map_err(|e| RpcError::Transport(format!("DDS envelope deserialization failed: {e}")))
    }
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
                // ---
                log_debug!(
                    "{}: Cmd::Publish called for topic {topic}",
                    actor.transport_id
                );

                let result = actor.handle_publish(topic, env).await;

                log_debug!(
                    "{}: Cmd::Publish sending result:{result:?}",
                    actor.transport_id
                );

                if resp.send(result).is_err() {
                    log_error!(
                        "{}: publish responder dropped before response could be sent",
                        actor.transport_id
                    );
                }
                ActorStep::Continue
            }
            Cmd::Subscribe { topic, resp } => {
                // ---
                log_debug!(
                    "{}: Cmd::Subscribe called for topic {topic}",
                    actor.transport_id
                );
                let result = actor.handle_subscribe(&topic).await;
                log_debug!(
                    "{}: Cmd::Subscribe sending result:{result:?} for topic {topic}",
                    actor.transport_id
                );
                let _ = resp.send(result);

                ActorStep::Continue
            }
            Cmd::Close { resp } => {
                // ---
                log_debug!("{}: Cmd::Close called", actor.transport_id);

                actor.handle_close().await;

                if resp.send(Ok(())).is_err() {
                    log_debug!(
                        "{}: Cmd::Close responder dropped before response could be sent",
                        actor.transport_id
                    );
                }

                ActorStep::Stop
            }
        }
    }
}

/// DDS-based implementation of the `Transport` trait.
///
/// Represents a single DomainParticipant and provides reliable,
/// non-durable message delivery consistent with other transports' semantics.
pub struct DustddsTransport {
    // ---
    transport_id: String,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

impl DustddsTransport {
    // ---

    /// Creates a new dust_dds transport with the given `DomainParticipant`.
    pub fn create(
        transport_id: impl Into<String>,
        participant: DomainParticipantAsync<StdRuntime>,
    ) -> TransportPtr {
        // ---

        let transport_id = transport_id.into();

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let subscribers = Arc::new(RwLock::new(HashMap::new()));
        let tasks = Arc::new(RwLock::new(Vec::new()));

        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        let actor = DdsActor {
            transport_id: transport_id.clone(),
            participant,
            cmd_rx,
            subscribers: Arc::clone(&subscribers),
            shutdown_tx,
            writers: HashMap::new(),
            reader_tasks: Vec::new(),
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
    participant: DomainParticipantAsync<StdRuntime>,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    shutdown_tx: watch::Sender<bool>,
    writers: HashMap<String, DataWriterAsync<StdRuntime, DdsEnvelope>>,
    reader_tasks: Vec<JoinHandle<()>>,
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

        // Request shutdown for all per-topic reader tasks and wait for them to exit.
        let _ = self.shutdown_tx.send(true);

        for handle in self.reader_tasks {
            let _ = handle.await;
        }

        log_info!("{}: DDS actor stopped", self.transport_id);
    }

    async fn handle_publish(&mut self, topic: String, env: Envelope) -> Result<()> {
        // ---
        let tid = self.transport_id.as_str();

        log_debug!("{tid}: handle_publish() called for topic {topic}");

        // Get or create DataWriter for this topic
        if !self.writers.contains_key(&topic) {
            self.create_writer(&topic).await?;
        }

        let tid = self.transport_id.as_str();

        let writer = self.writers.get(&topic).ok_or_else(|| {
            RpcError::Transport(format!("DataWriter not found for topic {topic}"))
        })?;

        // Wait for at least one matched reader
        wait_for_matched_reader(tid, &topic, writer, 5).await?;

        // Convert Envelope to DdsEnvelope for DDS transmission
        let dds_env = DdsEnvelope::try_from(&env)?;

        writer.write(dds_env, None).await.map_err(|e| {
            let msg = format!("{tid}: write failed for topic {topic}: {e:?}");
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?;

        log_debug!("{}: published message on topic {topic}", tid);
        Ok(())
    }

    async fn create_writer(&mut self, topic: &str) -> Result<()> {
        // ---
        let tid = self.transport_id.as_str();

        log_debug!("{tid}: create_writer called for topic {topic}...");
        log_debug!("{tid}: creating DataWriter for topic {topic}, with typename:{DDS_TYPE_NAME}");

        // Create topic
        let topic_obj = self
            .participant
            .create_topic::<DdsEnvelope>(
                topic,
                DDS_TYPE_NAME, // Constant type name for all DdsEnvelope topics
                QosKind::Default,
                None::<()>, // No listener
                &[],        // No status mask
            )
            .await
            .map_err(|e| RpcError::Transport(format!("create_topic failed: {e:?}")))?;

        // Create publisher
        let publisher = self
            .participant
            .create_publisher(QosKind::Default, None::<()>, &[])
            .await
            .map_err(|e| RpcError::Transport(format!("create_publisher failed: {e:?}")))?;

        // Build custom QoS for RPC semantics
        let writer_qos = build_rpc_writer_qos();

        // Create DataWriter
        let writer = publisher
            .create_datawriter::<DdsEnvelope>(
                &topic_obj,
                QosKind::Specific(writer_qos),
                None::<()>,
                &[],
            )
            .await
            .map_err(|e| RpcError::Transport(format!("create_datawriter failed: {e:?}")))?;

        let status = writer.get_publication_matched_status().await.map_err(|e| {
            RpcError::Transport(format!(
                "create_datawriter (get_publication_matched_status) failed: {e:?}"
            ))
        })?;

        log_debug!(
            "{tid}: publication matched count = {:?}",
            status.current_count
        );
        log_debug!("{tid}: writer created for topic {topic}");

        self.writers.insert(topic.to_string(), writer);
        log_debug!("{tid}: creating DataWriter for topic {topic}...SUCCESS");
        Ok(())
    }

    async fn handle_subscribe(&mut self, topic: &String) -> Result<()> {
        // ---

        log_debug!(
            "{}: handle_subscribe() called for topic {topic}",
            self.transport_id,
        );

        self.create_reader(topic).await
    }

    async fn create_reader(&mut self, topic: &str) -> Result<()> {
        // ---
        let tid = &self.transport_id;

        log_debug!("{tid}: creating DataReader for topic {topic}, with typename:{DDS_TYPE_NAME}");

        // Create topic
        let topic_obj = self
            .participant
            .create_topic::<DdsEnvelope>(
                topic,
                DDS_TYPE_NAME, // Constant type name for all DdsEnvelope topics
                QosKind::Default,
                None::<()>,
                &[],
            )
            .await
            .map_err(|e| RpcError::Transport(format!("create_topic failed: {e:?}")))?;

        // Create subscriber
        let subscriber = self
            .participant
            .create_subscriber(QosKind::Default, None::<()>, &[])
            .await
            .map_err(|e| RpcError::Transport(format!("create_subscriber failed: {e:?}")))?;

        // Build custom QoS for RPC semantics
        let reader_qos = build_rpc_reader_qos();

        // Create DataReader
        log_debug!("{tid}: calling subscriber.create_datareader for topic {topic}");
        let reader = subscriber
            .create_datareader::<DdsEnvelope>(
                &topic_obj,
                QosKind::Specific(reader_qos),
                None::<()>,
                &[],
            )
            .await
            .map_err(|e| RpcError::Transport(format!("create_datareader failed: {e:?}")))?;

        log_debug!("{tid}: created DataReader for topic {topic}...done");

        // Spawn task to poll this reader
        let transport_id = self.transport_id.clone();
        let topic_name = topic.to_string();
        let subscribers = Arc::clone(&self.subscribers);
        let shutdown_rx = self.shutdown_tx.subscribe();

        log_debug!(
            "{tid}: spawning task to poll DataReader for topic {topic} (with entrypoint run_topic_reader)"
        );

        let handle = tokio::spawn(async move {
            run_topic_reader(transport_id, topic_name, reader, subscribers, shutdown_rx).await;
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

/// Runs a per-topic reader task that waits for DDS data availability and fans out samples to local subscribers.
async fn run_topic_reader(
    transport_id: String,
    topic: String,
    reader: DataReaderAsync<StdRuntime, DdsEnvelope>,
    subscribers: SubscriberMap,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    log_debug!("{transport_id}: starting reader task for topic {topic}");

    // Configure a status condition that triggers when new data is available on this reader.
    let status_condition = reader.get_statuscondition();

    if let Err(e) = status_condition
        .set_enabled_statuses(&[StatusKind::DataAvailable])
        .await
    {
        log_error!("{transport_id}: set_enabled_statuses failed for topic {topic}: {e:?}");
        return;
    }

    let mut waitset = WaitSetAsync::new();
    if let Err(e) = waitset
        .attach_condition(ConditionAsync::StatusCondition(status_condition))
        .await
    {
        log_error!("{transport_id}: attach_condition failed for topic {topic}: {e:?}");
        return;
    }

    // NOTE: This is DDS timeout/duration. It is not not std or tokio versions.
    let one_day = DdsDuration::new(86400, 0);

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                log_debug!("{transport_id}: reader task shutdown requested for topic {topic}");
                break;
            }

            res = waitset.wait(one_day) => {
                let triggered = match res {
                    Ok(v) => v,
                    Err(e) => {
                        log_error!("{transport_id}: waitset.wait failed for topic {topic}: {e:?}");
                        break;
                    }
                };

                for cond in triggered {
                    match cond.get_trigger_value().await {
                        Ok(true) => {
                            if let Err(e) = drain_reader(&transport_id, &topic, &reader, &subscribers).await {
                                log_error!("{transport_id}: drain_reader error for topic {topic}: {e:?}");
                                break;
                            }
                        }
                        Ok(false) => { /* ignore */ }
                        Err(e) => {
                            log_error!("{transport_id}: get_trigger_value failed for topic {topic}: {e:?}");
                        }
                    }
                }
            }
        }
    }

    log_debug!("{transport_id}: reader task stopped for topic {topic}");
}

async fn drain_reader(
    transport_id: &str,
    topic: &str,
    reader: &DataReaderAsync<StdRuntime, DdsEnvelope>,
    subscribers: &SubscriberMap,
) -> Result<()> {
    // ---
    loop {
        let samples = match reader
            .take(
                10,
                &[SampleStateKind::NotRead],
                ANY_VIEW_STATE,
                ANY_INSTANCE_STATE,
            )
            .await
        {
            Ok(samples) => samples,

            Err(DdsError::NoData) => {
                // Normal drain completion
                break;
            }

            Err(e) => {
                return Err(RpcError::Transport(format!(
                    "take error on topic {topic}: {e:?}"
                )));
            }
        };

        log_info!(
            "{transport_id}: received {} samples on topic {topic}",
            samples.len(),
        );

        for (i, sample) in samples.iter().enumerate() {
            log_debug!(
                "{transport_id}: sample {i}: data.is_some()={}",
                sample.data.is_some()
            );

            if let Some(dds_env) = &sample.data {
                log_debug!(
                    "{transport_id}: DdsEnvelope key={}, data_len={}",
                    dds_env.topic,
                    dds_env.data.len()
                );

                match serde_json::from_slice::<Envelope>(&dds_env.data) {
                    Ok(env) => {
                        log_info!("{transport_id}: successfully deserialized envelope");
                        handle_incoming(transport_id, topic, Arc::clone(subscribers), env).await;
                    }
                    Err(e) => {
                        log_error!("{transport_id}: deserialization error: {e:?}",);
                    }
                }
            }
        }
    }

    Ok(())
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

/// Builds QoS policies appropriate for RPC semantics for DataWriter.
///
/// - `Reliability::Reliable` - TCP-like delivery with retries (consistent with MQTT/AMQP)
/// - `History::KeepLast(1)` - Only latest message (prevents correlation confusion)
/// - `Durability::Volatile` - No persistence (ephemeral, point-to-point)
fn build_rpc_writer_qos() -> DataWriterQos {
    // ---

    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: DurationKind::Finite(DdsDuration::new(0, 0)),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(1),
        },
        durability: DurabilityQosPolicy {
            kind: DurabilityQosPolicyKind::Volatile,
        },
        ..Default::default()
    }
}

/// Builds QoS policies appropriate for RPC semantics for DataReader.
fn build_rpc_reader_qos() -> DataReaderQos {
    // ---

    DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: DurationKind::Finite(DdsDuration::new(0, 0)),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(1),
        },
        durability: DurabilityQosPolicy {
            kind: DurabilityQosPolicyKind::Volatile,
        },
        ..Default::default()
    }
}

#[async_trait::async_trait]
impl Transport for DustddsTransport {
    // ---

    fn transport_id(&self) -> &str {
        // ---
        &self.transport_id
    }

    async fn publish(&self, env: Envelope) -> Result<()> {
        // ---
        let topic = env.address.0.to_string();

        log_debug!(
            "{}: publish() enqueue for topic {}",
            self.transport_id,
            topic
        );

        let (tx, rx) = oneshot::channel();

        self.cmd_tx
            .send(Cmd::Publish {
                topic: topic.clone(),
                env,
                resp: tx,
            })
            .await
            .map_err(|e| {
                RpcError::Transport(format!(
                    "{}: actor command channel closed: {e}",
                    self.transport_id
                ))
            })?;

        log_debug!(
            "{}: publish() waiting for Publish response from actor for {topic}...",
            self.transport_id,
        );

        let status = rx.await.map_err(|e| {
            RpcError::Transport(format!(
                "{}: actor responder channel read failed: {e}",
                self.transport_id
            ))
        })?;

        log_debug!(
            "{}: publish() waiting for Publish response from actor for {topic}...done, status:{status:?}",
            self.transport_id,
        );
        Ok(())
    }

    async fn subscribe(&self, sub: Subscription) -> Result<SubscriptionHandle> {
        // ---

        let topic = sub.0.as_ref().to_string();
        log_debug!(
            "{}: subscribe() called for topic {topic}",
            self.transport_id,
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

/// Creates a dust_dds-based DDS transport from the given configuration.
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
    let domain_id = parse_domain_id(config.transport_uri.as_deref())?;

    // Get the singleton factory instance for StdRuntime
    let participant_factory = DomainParticipantFactoryAsync::get_instance();

    let participant = participant_factory
        .create_participant(domain_id as i32, QosKind::Default, None::<()>, &[])
        .await
        .map_err(|e| {
            let msg = format!("Failed to create DomainParticipant on domain {domain_id}: {e:?}");
            log_error!("{msg}");
            RpcError::Transport(msg)
        })?;

    log_info!(
        "{}: created DomainParticipant on domain {}",
        config.transport_id,
        domain_id
    );

    Ok(DustddsTransport::create(&config.transport_id, participant))
}

/// Parses domain ID from `transport_uri`.
///
/// Expected format: `dds:<domain_id>` (e.g., "dds:0", "dds:45")
/// Returns `Ok(0)` (default domain) when `uri` is `None`.
/// Returns `RpcError::Transport` if the URI format is invalid or domain ID cannot be parsed.
fn parse_domain_id(uri: Option<&str>) -> Result<u16> {
    // ---

    let Some(uri) = uri else {
        return Ok(0); // Default domain when not specified
    };

    let domain_str = uri.strip_prefix("dds:").ok_or_else(|| {
        RpcError::Transport(format!(
            "Invalid DDS URI format '{uri}', expected 'dds:<domain_id>'"
        ))
    })?;

    domain_str.parse().map_err(|_| {
        RpcError::Transport(format!(
            "Invalid DDS domain ID '{domain_str}', must be a number"
        ))
    })
}

async fn wait_for_matched_reader(
    tid: &str,
    topic: &str,
    writer: &DataWriterAsync<StdRuntime, DdsEnvelope>,
    timeout_secs: u64,
) -> Result<()> {
    // ---

    log_debug!("{tid}: wait_for_matched_reader, topic:{topic}");

    // Check FIRST - avoid race condition
    let status = writer.get_publication_matched_status().await.map_err(|e| {
        RpcError::Transport(format!("get_publication_matched_status failed: {e:?}"))
    })?;

    if status.current_count > 0 {
        log_debug!(
            "{tid}: already matched {} readers for topic {topic}",
            status.current_count
        );
        return Ok(()); // ← Server would return here and succeed!
    }

    // Only set up WaitSet if count is 0
    log_debug!("{tid}: wait_for_matched_reader, topic:{topic}");

    let status_condition = writer.get_statuscondition();
    status_condition
        .set_enabled_statuses(&[StatusKind::PublicationMatched])
        .await
        .map_err(|e| RpcError::Transport(format!("set_enabled_statuses failed: {e:?}")))?;

    let mut waitset = WaitSetAsync::new();
    waitset
        .attach_condition(ConditionAsync::StatusCondition(status_condition))
        .await
        .map_err(|e| RpcError::Transport(format!("attach_condition failed: {e:?}")))?;

    let timeout = DdsDuration::new(timeout_secs as i32, 0);

    // Wait for publication match
    waitset.wait(timeout).await.map_err(|e| {
        RpcError::Transport(format!(
            "{tid}: no matched readers for topic {topic} within {timeout_secs}s: {e:?}"
        ))
    })?;

    // Verify we actually have a match
    let status = writer.get_publication_matched_status().await.map_err(|e| {
        RpcError::Transport(format!("get_publication_matched_status failed: {e:?}"))
    })?;

    if status.current_count == 0 {
        return Err(RpcError::Transport(format!(
            "{tid}: no matched readers for topic {topic}"
        )));
    }

    log_debug!(
        "{tid}: matched {} readers for topic {topic}",
        status.current_count
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[test]
    fn test_dds_envelope_roundtrip() {
        // --
        let env = Envelope::request(
            crate::Address::from("hello"),
            "method".into(),
            bytes::Bytes::new(),
            "CorrelationId".into(),
            "reply_to".into(),
            "application/json".into(),
        );
        let dds_env = DdsEnvelope::try_from(&env).unwrap();
        let env2 = Envelope::try_from(dds_env).unwrap();
        assert_eq!(env.correlation_id, env2.correlation_id);
    }

    #[test]
    fn test_parse_domain_id_none() {
        assert_eq!(parse_domain_id(None).unwrap(), 0);
    }

    #[test]
    fn test_parse_domain_id_valid() {
        assert_eq!(parse_domain_id(Some("dds:0")).unwrap(), 0);
        assert_eq!(parse_domain_id(Some("dds:42")).unwrap(), 42);
        assert_eq!(parse_domain_id(Some("dds:65535")).unwrap(), 65535);
    }

    #[test]
    fn test_parse_domain_id_missing_prefix() {
        let result = parse_domain_id(Some("42"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RpcError::Transport(_)));
    }

    #[test]
    fn test_parse_domain_id_invalid_number() {
        let result = parse_domain_id(Some("dds:not_a_number"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_domain_id_overflow() {
        let result = parse_domain_id(Some("dds:99999"));
        assert!(result.is_err());
    }
}
