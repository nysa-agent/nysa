use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::models::ThreadState;

struct ProcessingLock {
    lock: Arc<Mutex<()>>,
    last_accessed: chrono::DateTime<Utc>,
}

/// Manager for Discord conversation threads
/// Handles creation, lifecycle, and revival of threads
pub struct ThreadManager {
    /// Active threads by Discord channel ID (threads from @mentions or /newthread)
    active_threads: Arc<RwLock<HashMap<u64, ThreadState>>>,
    /// Threads by thread UUID for cross-platform lookup
    threads_by_uuid: Arc<RwLock<HashMap<Uuid, ThreadState>>>,
    /// User to thread mapping for DMs
    user_threads: Arc<RwLock<HashMap<u64, Uuid>>>,
    /// Thread timeout duration (30 minutes of inactivity)
    thread_timeout: Duration,
    /// Per-channel processing locks to guarantee sequential message handling
    processing_locks: Arc<RwLock<HashMap<u64, ProcessingLock>>>,
}

/// Represents a thread creation context
#[derive(Debug, Clone)]
pub enum ThreadContext {
    /// Thread created from @mention in a guild channel
    Mention {
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
    },
    /// Thread created from /newthread command
    Command {
        guild_id: u64,
        channel_id: u64,
        thread_name: String,
    },
    /// Thread created in DMs
    DirectMessage { user_id: u64 },
    /// Thread revived from an old conversation
    Revival {
        original_thread_id: Uuid,
        channel_id: u64,
    },
}

impl Clone for ThreadManager {
    fn clone(&self) -> Self {
        Self {
            active_threads: Arc::clone(&self.active_threads),
            threads_by_uuid: Arc::clone(&self.threads_by_uuid),
            user_threads: Arc::clone(&self.user_threads),
            thread_timeout: self.thread_timeout,
            processing_locks: Arc::clone(&self.processing_locks),
        }
    }
}

impl ThreadManager {
    pub fn new() -> Self {
        let active_threads: Arc<RwLock<HashMap<u64, ThreadState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let threads_by_uuid: Arc<RwLock<HashMap<Uuid, ThreadState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let user_threads: Arc<RwLock<HashMap<u64, Uuid>>> = Arc::new(RwLock::new(HashMap::new()));
        let processing_locks: Arc<RwLock<HashMap<u64, ProcessingLock>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let threads_clone = Arc::clone(&active_threads);
        let uuid_clone = Arc::clone(&threads_by_uuid);
        let user_threads_clone = Arc::clone(&user_threads);
        let locks_clone = Arc::clone(&processing_locks);

        // Start cleanup task for inactive threads and stale locks
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;

                let now = Utc::now();
                let thread_timeout = Duration::minutes(30);

                let mut threads = threads_clone.write().await;
                let mut uuid_threads = uuid_clone.write().await;
                let mut user_map = user_threads_clone.write().await;

                // Find expired threads
                let expired: Vec<u64> = threads
                    .iter()
                    .filter(|(_, state)| now.signed_duration_since(state.last_message_at) > thread_timeout)
                    .map(|(id, _)| *id)
                    .collect();

                for channel_id in expired {
                    if let Some(state) = threads.remove(&channel_id) {
                        uuid_threads.remove(&state.id);
                        // Also remove from user_threads if it's a DM thread
                        if state.discord_channel_id == channel_id {
                            user_map.retain(|_, thread_id| *thread_id != state.id);
                        }
                        tracing::info!("Thread {} timed out and was archived", state.id);
                    }
                }

                // Clean up stale processing locks (not accessed in 1 hour)
                let lock_timeout = Duration::hours(1);
                let mut locks = locks_clone.write().await;
                locks.retain(|_, lock| now.signed_duration_since(lock.last_accessed) <= lock_timeout);
            }
        });

        Self {
            active_threads,
            threads_by_uuid,
            user_threads,
            thread_timeout: Duration::minutes(30),
            processing_locks,
        }
    }

    /// Create a new thread from @mention
    pub async fn create_from_mention(
        &self,
        discord_channel_id: u64,
        discord_message_id: u64,
        user_id: Uuid,
    ) -> ThreadState {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;

        let thread_state = ThreadState::new(discord_channel_id, user_id);
        let mut state = thread_state.clone();
        state.discord_thread_id = Some(discord_message_id);
        state.add_message(discord_message_id);

        threads.insert(discord_channel_id, state.clone());
        uuid_threads.insert(state.id, state.clone());

        tracing::info!(
            "Created thread {} from mention in channel {} for user {}",
            state.id,
            discord_channel_id,
            user_id
        );

        state
    }

    /// Create a thread from a reply chain (continuing conversation)
    pub async fn create_from_reply(
        &self,
        discord_channel_id: u64,
        parent_message_id: u64,
        user_id: Uuid,
    ) -> ThreadState {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;

        let thread_state = ThreadState::new(discord_channel_id, user_id);
        let mut state = thread_state.clone();
        state.discord_thread_id = Some(parent_message_id);
        state.add_message(parent_message_id);

        threads.insert(discord_channel_id, state.clone());
        uuid_threads.insert(state.id, state.clone());

        tracing::info!(
            "Created thread {} from reply in channel {} for user {}",
            state.id,
            discord_channel_id,
            user_id
        );

        state
    }

    /// Get or create a DM thread for a user
    pub async fn get_or_create_dm_thread(
        &self,
        user_discord_id: u64,
        user_uuid: Uuid,
    ) -> ThreadState {
        // Check if there's an existing DM thread
        let should_update = {
            let user_map = self.user_threads.read().await;
            if let Some(thread_id) = user_map.get(&user_discord_id) {
                let uuid_threads = self.threads_by_uuid.read().await;
                if let Some(state) = uuid_threads.get(thread_id) {
                    // Check if expired
                    let now = Utc::now();
                    if now.signed_duration_since(state.last_message_at) <= self.thread_timeout {
                        Some((state.discord_channel_id, state.id))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Update outside of the read locks
        if let Some((discord_channel_id, thread_id)) = should_update {
            let mut threads = self.active_threads.write().await;
            let mut uuid_threads = self.threads_by_uuid.write().await;

            if let Some(mut state) = threads.get(&discord_channel_id).cloned() {
                state.last_message_at = Utc::now();
                threads.insert(discord_channel_id, state.clone());
                uuid_threads.insert(thread_id, state.clone());
                return state;
            }
        }

        // Create new DM thread using a dedicated keyspace to avoid collisions with guild channels
        let discord_channel_id = user_discord_id | (1_u64 << 63);
        let thread_state = ThreadState::new(discord_channel_id, user_uuid);
        let state = thread_state.clone();

        {
            let mut threads = self.active_threads.write().await;
            let mut uuid_threads = self.threads_by_uuid.write().await;
            let mut user_map = self.user_threads.write().await;

            threads.insert(discord_channel_id, state.clone());
            uuid_threads.insert(state.id, state.clone());
            user_map.insert(user_discord_id, state.id);
        }

        tracing::info!(
            "Created new DM thread {} for Discord user {} (UUID: {})",
            state.id,
            user_discord_id,
            user_uuid
        );

        state
    }

    /// Revive an old thread from a reply
    pub async fn revive_thread(
        &self,
        original_thread_id: Uuid,
        new_discord_channel_id: u64,
    ) -> Option<ThreadState> {
        let old_state = {
            let uuid_threads = self.threads_by_uuid.read().await;
            uuid_threads.get(&original_thread_id).cloned()
        };

        if let Some(old_state) = old_state {
            let user_id = old_state.user_id;

            // Create new thread state with reference to old thread
            let mut new_state = ThreadState::new(new_discord_channel_id, user_id);
            new_state.is_active = true;

            let mut threads = self.active_threads.write().await;
            let mut uuid_threads = self.threads_by_uuid.write().await;

            threads.insert(new_discord_channel_id, new_state.clone());
            uuid_threads.insert(new_state.id, new_state.clone());

            tracing::info!(
                "Revived thread {} as new thread {} in channel {}",
                original_thread_id,
                new_state.id,
                new_discord_channel_id
            );

            Some(new_state)
        } else {
            None
        }
    }

    /// Get active thread by Discord channel ID
    pub async fn get_thread(&self, discord_channel_id: u64) -> Option<ThreadState> {
        let threads = self.active_threads.read().await;
        threads.get(&discord_channel_id).cloned()
    }

    /// Get thread by UUID
    pub async fn get_thread_by_uuid(&self, thread_id: Uuid) -> Option<ThreadState> {
        let threads = self.threads_by_uuid.read().await;
        threads.get(&thread_id).cloned()
    }

    /// Update last message timestamp
    pub async fn update_activity(&self, discord_channel_id: u64) {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;

        if let Some(mut state) = threads.get(&discord_channel_id).cloned() {
            state.last_message_at = Utc::now();
            threads.insert(discord_channel_id, state.clone());
            uuid_threads.insert(state.id, state);
        }
    }

    /// Add a message ID to an existing thread by channel key
    pub async fn add_message_to_thread(&self, discord_channel_id: u64, message_id: u64) {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;

        if let Some(mut state) = threads.get(&discord_channel_id).cloned() {
            state.add_message(message_id);
            state.last_message_at = Utc::now();
            threads.insert(discord_channel_id, state.clone());
            uuid_threads.insert(state.id, state);

            if let Some(updated) = threads.get(&discord_channel_id) {
                tracing::debug!(
                    "Added message {} to thread {} in channel {}, message_ids={:?}",
                    message_id,
                    updated.id,
                    discord_channel_id,
                    updated.message_ids
                );
            }
        } else {
            tracing::warn!(
                "No thread found in channel {} to add message {}",
                discord_channel_id,
                message_id
            );
        }
    }

    /// Mark thread as inactive (e.g., after explicit close)
    pub async fn close_thread(&self, discord_channel_id: u64) -> bool {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;
        let mut user_map = self.user_threads.write().await;

        if let Some(state) = threads.remove(&discord_channel_id) {
            uuid_threads.remove(&state.id);
            user_map.retain(|_, thread_id| *thread_id != state.id);

            tracing::info!("Closed thread {}", state.id);
            true
        } else {
            false
        }
    }

    /// Check if a message is in an active thread
    pub async fn is_in_active_thread(&self, discord_channel_id: u64) -> bool {
        let threads = self.active_threads.read().await;
        if let Some(state) = threads.get(&discord_channel_id) {
            let now = Utc::now();
            now.signed_duration_since(state.last_message_at) <= self.thread_timeout
        } else {
            false
        }
    }

    /// Get all active threads for a user
    pub async fn get_user_threads(&self, user_id: Uuid) -> Vec<ThreadState> {
        let threads = self.active_threads.read().await;
        threads
            .values()
            .filter(|t| t.user_id == user_id && t.is_active)
            .cloned()
            .collect()
    }

    /// Get count of active threads
    pub async fn active_thread_count(&self) -> usize {
        let threads = self.active_threads.read().await;
        threads.len()
    }

    /// Check if a reply is to a message in an active thread
    pub async fn check_reply_chain(&self, parent_message_id: u64) -> Option<ThreadState> {
        let threads = self.active_threads.read().await;
        tracing::debug!(
            "Checking reply chain for parent_message_id={}, active_threads={}",
            parent_message_id,
            threads.len()
        );

        for (channel_id, thread) in threads.iter() {
            tracing::debug!(
                "Thread {} in channel {}: message_ids={:?}, contains={}",
                thread.id,
                channel_id,
                thread.message_ids,
                thread.contains_message(parent_message_id)
            );
        }

        let result = threads
            .values()
            .find(|t| t.contains_message(parent_message_id))
            .cloned();

        if let Some(ref thread) = result {
            tracing::info!(
                "Found thread {} for parent_message_id {}",
                thread.id,
                parent_message_id
            );
        } else {
            tracing::warn!(
                "No thread found for parent_message_id {}",
                parent_message_id
            );
        }

        result
    }

    /// Add a message ID to an existing thread by thread UUID
    pub async fn add_message_to_thread_by_uuid(&self, thread_id: Uuid, message_id: u64) -> bool {
        let maybe_state = {
            let uuid_threads = self.threads_by_uuid.read().await;
            uuid_threads.get(&thread_id).cloned()
        };

        if let Some(state) = maybe_state {
            self.add_message_to_thread(state.discord_channel_id, message_id)
                .await;
            true
        } else {
            tracing::warn!(
                "No thread found by UUID {} to add message {}",
                thread_id,
                message_id
            );
            false
        }
    }

    /// Update activity for a thread by thread UUID
    pub async fn update_activity_by_uuid(&self, thread_id: Uuid) -> bool {
        let maybe_state = {
            let uuid_threads = self.threads_by_uuid.read().await;
            uuid_threads.get(&thread_id).cloned()
        };

        if let Some(state) = maybe_state {
            self.update_activity(state.discord_channel_id).await;
            true
        } else {
            tracing::warn!("No thread found by UUID {} to update activity", thread_id);
            false
        }
    }

    /// Ensure a per-channel processing mutex exists and return it.
    /// Callers can lock it to enforce sequential processing for a channel.
    pub async fn get_or_create_processing_lock(&self, channel_id: u64) -> Arc<Mutex<()>> {
        {
            let mut locks = self.processing_locks.write().await;
            if let Some(lock) = locks.get_mut(&channel_id) {
                lock.last_accessed = Utc::now();
                return Arc::clone(&lock.lock);
            }

            let new_lock = ProcessingLock {
                lock: Arc::new(Mutex::new(())),
                last_accessed: Utc::now(),
            };
            let lock = Arc::clone(&new_lock.lock);
            locks.insert(channel_id, new_lock);
            lock
        }
    }

    /// Register a conversational thread against a specific Discord channel key.
    /// Useful for externally-created Discord threads (e.g. /newthread).
    pub async fn register_channel_thread(
        &self,
        discord_channel_id: u64,
        user_id: Uuid,
        seed_message_id: Option<u64>,
    ) -> ThreadState {
        let mut threads = self.active_threads.write().await;
        let mut uuid_threads = self.threads_by_uuid.write().await;

        let mut state = ThreadState::new(discord_channel_id, user_id);
        if let Some(message_id) = seed_message_id {
            state.discord_thread_id = Some(message_id);
            state.add_message(message_id);
        }

        threads.insert(discord_channel_id, state.clone());
        uuid_threads.insert(state.id, state.clone());

        tracing::info!(
            "Registered conversational thread {} for channel {}",
            state.id,
            discord_channel_id
        );

        state
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}
