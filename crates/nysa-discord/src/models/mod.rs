use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub mod linking_code;
pub mod user;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelMode {
    Disabled,
    EvaluateAll,
    Thread,
    Active,
}

impl Default for ChannelMode {
    fn default() -> Self {
        ChannelMode::Thread
    }
}

impl std::fmt::Display for ChannelMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelMode::Disabled => write!(f, "disabled"),
            ChannelMode::EvaluateAll => write!(f, "evaluate_all"),
            ChannelMode::Thread => write!(f, "thread"),
            ChannelMode::Active => write!(f, "active"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DmMode {
    Reactive,
    Proactive,
}

impl Default for DmMode {
    fn default() -> Self {
        DmMode::Reactive
    }
}

impl std::fmt::Display for DmMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DmMode::Reactive => write!(f, "reactive"),
            DmMode::Proactive => write!(f, "proactive"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub token: String,
    pub application_id: u64,
    pub default_mode: ChannelMode,
    pub proactive_min: i64,
    pub proactive_max: i64,
    pub dm_mode: DmMode,
    pub unauth_message: UnauthMessageTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnauthMessageTemplate {
    pub title: String,
    pub description: String,
    pub color: i32,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            application_id: 0,
            default_mode: ChannelMode::Thread,
            proactive_min: 60,
            proactive_max: 240,
            dm_mode: DmMode::Reactive,
            unauth_message: UnauthMessageTemplate {
                title: "Authentication Required".to_string(),
                description: "Please authenticate with Nysa using `/auth` to start chatting."
                    .to_string(),
                color: 0xFF6B6B,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildConfig {
    pub guild_id: u64,
    pub mode: ChannelMode,
    pub proactive_min: i64,
    pub proactive_max: i64,
    pub channel_modes: HashMap<u64, ChannelMode>,
}

impl Default for GuildConfig {
    fn default() -> Self {
        Self {
            guild_id: 0,
            mode: ChannelMode::Thread,
            proactive_min: 60,
            proactive_max: 240,
            channel_modes: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadState {
    pub id: Uuid,
    pub discord_channel_id: u64,
    pub discord_thread_id: Option<u64>,
    pub user_id: Uuid,
    pub last_message_at: chrono::DateTime<chrono::Utc>,
    pub is_active: bool,
    /// Track all message IDs in this thread for reply detection
    pub message_ids: Vec<u64>,
}

impl ThreadState {
    pub fn new(discord_channel_id: u64, user_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            discord_channel_id,
            discord_thread_id: None,
            user_id,
            last_message_at: chrono::Utc::now(),
            is_active: true,
            message_ids: Vec::new(),
        }
    }

    /// Add a message ID to the thread's history
    pub fn add_message(&mut self, message_id: u64) {
        if !self.message_ids.contains(&message_id) {
            self.message_ids.push(message_id);
            // Keep only last 100 message IDs to prevent unbounded growth
            if self.message_ids.len() > 100 {
                self.message_ids.remove(0);
            }
        }
    }

    /// Check if a message ID is part of this thread
    pub fn contains_message(&self, message_id: u64) -> bool {
        self.message_ids.contains(&message_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveState {
    pub user_id: Uuid,
    pub last_message_at: chrono::DateTime<chrono::Utc>,
    pub min_interval_seconds: i64,
    pub max_interval_seconds: i64,
}

impl ProactiveState {
    pub fn new(user_id: Uuid, min_seconds: i64, max_seconds: i64) -> Self {
        Self {
            user_id,
            last_message_at: chrono::Utc::now(),
            min_interval_seconds: min_seconds,
            max_interval_seconds: max_seconds,
        }
    }

    pub fn should_send(&self) -> bool {
        let now = chrono::Utc::now();
        let elapsed = (now - self.last_message_at).num_seconds();

        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_interval = rng.gen_range(self.min_interval_seconds..self.max_interval_seconds);

        elapsed >= random_interval
    }
}
