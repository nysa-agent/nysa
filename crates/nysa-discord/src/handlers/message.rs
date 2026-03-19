use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::{ChannelMode, DiscordConfig, DmMode, ProactiveState, ThreadState};

pub struct DiscordMessageHandler {
    config: DiscordConfig,
    active_threads: Arc<RwLock<HashMap<Uuid, ThreadState>>>,
    dm_threads: Arc<RwLock<HashMap<u64, Uuid>>>,
    proactive_states: Arc<RwLock<HashMap<u64, ProactiveState>>>,
    channel_modes: Arc<RwLock<HashMap<u64, ChannelMode>>>,
}

impl DiscordMessageHandler {
    pub fn new(config: DiscordConfig) -> Self {
        let active_threads: Arc<RwLock<HashMap<Uuid, ThreadState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let dm_threads: Arc<RwLock<HashMap<u64, Uuid>>> = Arc::new(RwLock::new(HashMap::new()));
        let proactive_states: Arc<RwLock<HashMap<u64, ProactiveState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let channel_modes: Arc<RwLock<HashMap<u64, ChannelMode>>> =
            Arc::new(RwLock::new(HashMap::new()));

        Self {
            config,
            active_threads,
            dm_threads,
            proactive_states,
            channel_modes,
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

    /// Get channel mode for a specific channel
    pub async fn get_channel_mode(&self, channel_id: u64, guild_id: Option<u64>) -> ChannelMode {
        // Check for channel-specific override
        let modes = self.channel_modes.read().await;
        if let Some(mode) = modes.get(&channel_id) {
            return *mode;
        }
        drop(modes);

        // Check for guild-specific config if available
        if let Some(_guild_id) = guild_id {
            // TODO: Load from guild config in database
        }

        // Return default
        self.config.default_mode
    }

    /// Set channel mode
    pub async fn set_channel_mode(&self, channel_id: u64, mode: ChannelMode) {
        let mut modes = self.channel_modes.write().await;
        modes.insert(channel_id, mode);
    }

    /// Check if we should respond proactively to this user
    pub async fn should_respond_proactively(&self, user_id: u64) -> bool {
        let states = self.proactive_states.read().await;

        if let Some(state) = states.get(&user_id) {
            // Check if enough time has passed
            state.should_send()
        } else {
            // No proactive state yet - check if we should start
            // Only in active mode or proactive DM mode
            matches!(self.config.dm_mode, DmMode::Proactive)
        }
    }

    /// Update proactive state for a user
    pub async fn update_proactive_state(&self, user_id: u64, user_uuid: Uuid) {
        let mut states = self.proactive_states.write().await;

        if let Some(state) = states.get_mut(&user_id) {
            state.last_message_at = chrono::Utc::now();
        } else {
            let state = ProactiveState::new(
                user_uuid,
                self.config.proactive_min,
                self.config.proactive_max,
            );
            states.insert(user_id, state);
        }
    }

    /// Register a message for proactive tracking
    pub async fn register_proactive_message(&self, user_id: u64, user_uuid: Uuid) {
        let mut states = self.proactive_states.write().await;

        let state = ProactiveState::new(
            user_uuid,
            self.config.proactive_min,
            self.config.proactive_max,
        );
        states.insert(user_id, state);
    }

    /// Get proactive state for a user
    pub async fn get_proactive_state(&self, user_id: u64) -> Option<ProactiveState> {
        let states = self.proactive_states.read().await;
        states.get(&user_id).cloned()
    }

    /// Remove proactive state
    pub async fn remove_proactive_state(&self, user_id: u64) {
        let mut states = self.proactive_states.write().await;
        states.remove(&user_id);
    }

    /// Get unauth message template
    pub fn unauth_embed(&self) -> &crate::models::UnauthMessageTemplate {
        &self.config.unauth_message
    }

    /// Get or create a thread for a user in a channel
    pub async fn get_or_create_thread(
        &self,
        discord_channel_id: u64,
        user_uuid: Uuid,
    ) -> ThreadState {
        // Check if there's already an active thread in this channel for this user
        let threads = self.active_threads.read().await;
        for thread in threads.values() {
            if thread.discord_channel_id == discord_channel_id && thread.user_id == user_uuid
                && thread.is_active
            {
                return thread.clone();
            }
        }
        drop(threads);

        // Create new thread
        let thread = ThreadState::new(discord_channel_id, user_uuid);
        let mut threads = self.active_threads.write().await;
        threads.insert(thread.id, thread.clone());

        thread
    }

    /// Get thread by ID
    pub async fn get_thread(&self, thread_id: Uuid) -> Option<ThreadState> {
        let threads = self.active_threads.read().await;
        threads.get(&thread_id).cloned()
    }

    /// Update thread activity
    pub async fn update_thread_activity(&self, thread_id: Uuid) {
        let mut threads = self.active_threads.write().await;
        if let Some(thread) = threads.get_mut(&thread_id) {
            thread.last_message_at = chrono::Utc::now();
        }
    }

    /// Close a thread
    pub async fn close_thread(&self, thread_id: Uuid) -> bool {
        let mut threads = self.active_threads.write().await;
        if let Some(thread) = threads.get_mut(&thread_id) {
            thread.is_active = false;
            true
        } else {
            false
        }
    }

    /// Get active thread count
    pub async fn active_thread_count(&self) -> usize {
        let threads = self.active_threads.read().await;
        threads.values().filter(|t| t.is_active).count()
    }

    /// Get all active threads for a user
    pub async fn get_user_threads(&self, user_uuid: Uuid) -> Vec<ThreadState> {
        let threads = self.active_threads.read().await;
        threads
            .values()
            .filter(|t| t.user_id == user_uuid && t.is_active)
            .cloned()
            .collect()
    }
}

impl Clone for DiscordMessageHandler {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            active_threads: Arc::clone(&self.active_threads),
            dm_threads: Arc::clone(&self.dm_threads),
            proactive_states: Arc::clone(&self.proactive_states),
            channel_modes: Arc::clone(&self.channel_modes),
        }
    }
}
