use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::models::{ChannelMode, DmMode, DiscordConfig, ProactiveState};

pub struct DiscordMessageHandler {
    config: DiscordConfig,
    active_threads: Arc<RwLock<HashMap<Uuid, crate::models::ThreadState>>>,
    dm_threads: Arc<RwLock<HashMap<u64, Uuid>>>,
    proactive_states: Arc<RwLock<HashMap<u64, ProactiveState>>>,
}

impl DiscordMessageHandler {
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            active_threads: Arc::new(RwLock::new(HashMap::new())),
            dm_threads: Arc::new(RwLock::new(HashMap::new())),
            proactive_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_dm_thread(&self, user_id: u64) -> Uuid {
        let thread_id = Uuid::new_v4();
        let mut dm_threads = self.dm_threads.write().await;
        dm_threads.insert(user_id, thread_id);
        thread_id
    }

    pub async fn get_or_create_dm_thread(&self, user_id: u64) -> Uuid {
        {
            let dm_threads = self.dm_threads.read().await;
            if let Some(thread_id) = dm_threads.get(&user_id) {
                return *thread_id;
            }
        }
        self.create_dm_thread(user_id).await
    }

    pub async fn should_respond_proactively(&self, _user_id: u64) -> bool {
        false
    }

    pub async fn update_proactive_state(&self, _user_id: u64) {
    }

    pub fn unauth_embed(&self) -> &crate::models::UnauthMessageTemplate {
        &self.config.unauth_message
    }
}
