use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::models::ProactiveState;

pub struct ProactiveManager {
    _states: Arc<RwLock<HashMap<u64, ProactiveState>>>,
}

impl ProactiveManager {
    pub fn new(_min_seconds: i64, _max_seconds: i64) -> Self {
        Self {
            _states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_user(&self, _user_id: Uuid) {
    }

    pub async fn record_message(&self, _user_id: Uuid) {
    }

    pub async fn should_send_message(&self, _user_id: Uuid) -> bool {
        false
    }
}

impl Default for ProactiveManager {
    fn default() -> Self {
        Self::new(60, 240)
    }
}
