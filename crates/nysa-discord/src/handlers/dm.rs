use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};

use crate::models::DmMode;

const DM_THREAD_TIMEOUT_MINUTES: i64 = 30;

pub struct DmHandler {
    mode: DmMode,
    active_dm_threads: Arc<RwLock<HashMap<u64, DmThreadState>>>,
}

#[derive(Debug, Clone)]
pub struct DmThreadState {
    pub thread_id: Uuid,
    pub user_id: u64,
    pub last_activity: DateTime<Utc>,
    pub is_active: bool,
}

impl Clone for DmHandler {
    fn clone(&self) -> Self {
        Self {
            mode: self.mode,
            active_dm_threads: Arc::clone(&self.active_dm_threads),
        }
    }
}

impl DmHandler {
    pub fn new(mode: DmMode) -> Self {
        let active_dm_threads: Arc<RwLock<HashMap<u64, DmThreadState>>> = Arc::new(RwLock::new(HashMap::new()));
        let threads_clone = Arc::clone(&active_dm_threads);

        // Start cleanup task for inactive DM threads
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                
                let now = Utc::now();
                let timeout = Duration::minutes(DM_THREAD_TIMEOUT_MINUTES);
                
                let mut threads = threads_clone.write().await;
                threads.retain(|_, state| {
                    if now.signed_duration_since(state.last_activity) > timeout {
                        tracing::info!("DM thread {} timed out after {} minutes", state.thread_id, DM_THREAD_TIMEOUT_MINUTES);
                        false
                    } else {
                        true
                    }
                });
            }
        });

        Self {
            mode,
            active_dm_threads,
        }
    }

    /// Get or create a DM thread for a user
    pub async fn get_or_create_thread(&self, user_id: u64, user_uuid: Uuid) -> DmThreadState {
        let mut threads = self.active_dm_threads.write().await;
        
        if let Some(state) = threads.get(&user_id) {
            let mut state = state.clone();
            state.last_activity = Utc::now();
            threads.insert(user_id, state.clone());
            return state;
        }

        let thread_id = Uuid::new_v4();
        let state = DmThreadState {
            thread_id,
            user_id,
            last_activity: Utc::now(),
            is_active: true,
        };

        threads.insert(user_id, state.clone());
        tracing::info!("Created new DM thread {} for user {} (uuid: {})", thread_id, user_id, user_uuid);
        
        state
    }

    /// Revive an old DM thread if it exists but is inactive
    pub async fn revive_thread(&self, user_id: u64) -> Option<DmThreadState> {
        let mut threads = self.active_dm_threads.write().await;
        
        if let Some(mut state) = threads.get(&user_id).cloned() {
            state.is_active = true;
            state.last_activity = Utc::now();
            threads.insert(user_id, state.clone());
            tracing::info!("Revived DM thread {} for user {}", state.thread_id, user_id);
            return Some(state);
        }

        None
    }

    /// Check if we should respond to a DM message based on mode
    pub async fn should_respond(&self, user_id: u64, is_proactive_eligible: bool) -> bool {
        match self.mode {
            DmMode::Reactive => {
                // Always respond when messaged
                true
            }
            DmMode::Proactive => {
                // Respond to messages, and also proactively if eligible
                let threads = self.active_dm_threads.read().await;
                if let Some(state) = threads.get(&user_id) {
                    // If there's an active conversation, respond
                    state.is_active || is_proactive_eligible
                } else {
                    // No active thread, allow proactive
                    is_proactive_eligible
                }
            }
        }
    }

    /// Mark thread as inactive (after responding)
    pub async fn mark_inactive(&self, user_id: u64) {
        let mut threads = self.active_dm_threads.write().await;
        if let Some(state) = threads.get_mut(&user_id) {
            state.is_active = false;
        }
    }

    /// Update last activity timestamp
    pub async fn update_activity(&self, user_id: u64) {
        let mut threads = self.active_dm_threads.write().await;
        if let Some(state) = threads.get_mut(&user_id) {
            state.last_activity = Utc::now();
        }
    }

    /// Get active DM thread count
    pub async fn active_thread_count(&self) -> usize {
        let threads = self.active_dm_threads.read().await;
        threads.len()
    }

    /// Get thread state for a user
    pub async fn get_thread(&self, user_id: u64) -> Option<DmThreadState> {
        let threads = self.active_dm_threads.read().await;
        threads.get(&user_id).cloned()
    }
}

impl Default for DmHandler {
    fn default() -> Self {
        Self::new(DmMode::Reactive)
    }
}
