use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

pub trait Event: Send + Sync + Clone + 'static {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReceived {
    pub source: MessageSource,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Event for MessageReceived {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageToSend {
    pub target: MessageTarget,
    pub content: String,
}

impl Event for MessageToSend {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageSource {
    Discord {
        channel_id: u64,
        author_id: u64,
    },
    Minecraft {
        player_uuid: Uuid,
        player_name: String,
    },
    Cli,
    Api,
    Custom {
        source_type: String,
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageTarget {
    Discord { channel_id: u64 },
    Minecraft { player_uuid: Uuid },
    Broadcast,
    Cli,
    Api { session_id: String },
    Custom { target_type: String, id: String },
}

const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

pub struct EventBus {
    channels: parking_lot::RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            channels: Default::default(),
        }
    }

    pub fn publish<E: Event>(&self, event: E) {
        let type_id = TypeId::of::<E>();

        let channels = self.channels.read();
        if let Some(sender) = channels.get(&type_id) {
            if let Some(tx) = sender.downcast_ref::<broadcast::Sender<E>>() {
                let _ = tx.send(event);
            }
        }
    }

    pub fn subscribe<E: Event>(&self) -> broadcast::Receiver<E> {
        let type_id = TypeId::of::<E>();

        {
            let channels = self.channels.read();
            if let Some(sender) = channels.get(&type_id) {
                if let Some(tx) = sender.downcast_ref::<broadcast::Sender<E>>() {
                    return tx.subscribe();
                }
            }
        }

        let mut channels = self.channels.write();
        let (tx, rx) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        channels.insert(type_id, Box::new(tx));
        rx
    }

    pub fn has_subscribers<E: Event>(&self) -> bool {
        let type_id = TypeId::of::<E>();
        let channels = self.channels.read();
        channels.contains_key(&type_id)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            channels: parking_lot::RwLock::new(HashMap::new()),
        }
    }
}

pub type SharedEventBus = Arc<EventBus>;
