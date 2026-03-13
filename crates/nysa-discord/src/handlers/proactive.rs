use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;
use rand::Rng;

use crate::models::ProactiveState;

pub struct ProactiveManager {
    states: Arc<RwLock<HashMap<u64, ProactiveState>>>,
    min_seconds: i64,
    max_seconds: i64,
}

impl Clone for ProactiveManager {
    fn clone(&self) -> Self {
        Self {
            states: Arc::clone(&self.states),
            min_seconds: self.min_seconds,
            max_seconds: self.max_seconds,
        }
    }
}

impl ProactiveManager {
    pub fn new(min_seconds: i64, max_seconds: i64) -> Self {
        let states: Arc<RwLock<HashMap<u64, ProactiveState>>> = Arc::new(RwLock::new(HashMap::new()));
        let states_clone = Arc::clone(&states);

        // Start cleanup task for old proactive states
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;
                
                let cutoff = Utc::now() - chrono::Duration::hours(24);
                let mut states = states_clone.write().await;
                
                states.retain(|_, state| state.last_message_at > cutoff);
            }
        });

        Self {
            states,
            min_seconds,
            max_seconds,
        }
    }

    /// Register a user for proactive messaging
    pub async fn register_user(&self, user_id: Uuid, discord_user_id: u64) {
        let state = ProactiveState::new(user_id, self.min_seconds, self.max_seconds);
        let mut states = self.states.write().await;
        states.insert(discord_user_id, state);
    }

    /// Record a message from a user (resets the timer)
    pub async fn record_message(&self, discord_user_id: u64) {
        let mut states = self.states.write().await;
        
        if let Some(state) = states.get_mut(&discord_user_id) {
            state.last_message_at = Utc::now();
        }
    }

    /// Check if we should send a proactive message to this user
    pub async fn should_send_message(&self, discord_user_id: u64) -> bool {
        let states = self.states.read().await;
        
        if let Some(state) = states.get(&discord_user_id) {
            let now = Utc::now();
            let elapsed = (now - state.last_message_at).num_seconds();
            
            // Generate random interval for this check
            let mut rng = rand::thread_rng();
            let random_interval = rng.gen_range(state.min_interval_seconds..state.max_interval_seconds);
            
            elapsed >= random_interval
        } else {
            false
        }
    }

    /// Get proactive state for a user
    pub async fn get_state(&self, discord_user_id: u64) -> Option<ProactiveState> {
        let states = self.states.read().await;
        states.get(&discord_user_id).cloned()
    }

    /// Remove a user from proactive messaging
    pub async fn unregister_user(&self, discord_user_id: u64) {
        let mut states = self.states.write().await;
        states.remove(&discord_user_id);
    }

    /// Get count of users being tracked
    pub async fn user_count(&self) -> usize {
        let states = self.states.read().await;
        states.len()
    }

    /// Update proactive intervals for a user
    pub async fn update_intervals(&self, discord_user_id: u64, min_seconds: i64, max_seconds: i64) {
        let mut states = self.states.write().await;
        
        if let Some(state) = states.get_mut(&discord_user_id) {
            state.min_interval_seconds = min_seconds;
            state.max_interval_seconds = max_seconds;
        }
    }
}

impl Default for ProactiveManager {
    fn default() -> Self {
        Self::new(60, 240)
    }
}
