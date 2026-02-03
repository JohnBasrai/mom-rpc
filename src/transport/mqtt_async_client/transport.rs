use std::sync::Arc;

#[allow(unused_imports)]
use crate::{
    //
    Address,
    Envelope,
    PublishOptions,
    Result,
    SubscribeOptions,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};

use super::SubscriptionManager;

/// Concrete Transport backed by mqtt-async-client.
///
/// All MQTT-specific concerns (topics, client state, callbacks, etc.)
/// are contained within this type.
struct MqttAsyncClientTransport {
    // ---
    transport_id: String,
    subscriptions: SubscriptionManager,
}

#[async_trait::async_trait]
impl Transport for MqttAsyncClientTransport {
    // ---
    fn transport_id(&self) -> &str {
        &self.transport_id
    }

    async fn publish(&self, env: Envelope, _opts: PublishOptions) -> Result<()> {
        // ---
        // TODO: For MQTT implementation:
        // - map Address -> MQTT topic
        // - serialize Envelope metadata into MQTT properties
        // - publish via MQTT client
        //
        // For now, simulate fanout using subscription manager.
        // This allows testing the RPC layer even without MQTT.
        self.subscriptions.fanout(&env, &self.transport_id).await;

        Ok(())
    }

    async fn subscribe(
        &self,
        sub: Subscription,
        _opts: SubscribeOptions,
    ) -> Result<SubscriptionHandle> {
        // ---
        #[cfg(feature = "logging")]
        log::debug!("{}: subscribe to {:?}", self.transport_id(), sub);

        // TODO: For MQTT implementation:
        // - map Subscription -> MQTT topic filter
        // - subscribe via MQTT client
        // - forward incoming MQTT messages to subscription manager
        //
        // For now, use subscription manager directly.
        let rx = self.subscriptions.add(sub).await;

        Ok(SubscriptionHandle { inbox: rx })
    }

    async fn close(&self) -> Result<()> {
        // ---
        #[cfg(feature = "logging")]
        log::debug!("{}: closing transport...", self.transport_id());

        // TODO: For MQTT implementation:
        // - gracefully disconnect MQTT client
        // - wait for pending publishes/acks
        //
        // For now, just clear subscriptions.
        self.subscriptions.clear().await;

        Ok(())
    }
}

/// Create an MQTT-based transport using mqtt-async-client.
///
/// This is the ONLY symbol exposed from this module.
pub async fn create_transport(transport_id: &str) -> Result<TransportPtr> {
    // ---
    // TODO:
    // - build mqtt-async-client options from config
    // - connect MQTT client
    // - set up background receive loop for incoming messages
    // - wire MQTT messages into subscription manager
    //
    #[cfg(feature = "logging")]
    log::debug!("{}: create mqtt_async_client transport", transport_id);

    let transport = MqttAsyncClientTransport {
        // ---
        transport_id: transport_id.to_owned(),
        subscriptions: SubscriptionManager::new(),
    };

    Ok(Arc::new(transport))
}
