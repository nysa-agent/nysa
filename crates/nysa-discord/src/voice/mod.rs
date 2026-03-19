use std::sync::Arc;
use tokio::sync::RwLock;

pub struct VoiceManager {
    active_connections: Arc<RwLock<Vec<VoiceConnection>>>,
}

struct VoiceConnection {
    guild_id: u64,
    channel_id: u64,
}

impl VoiceManager {
    pub fn new() -> Self {
        Self {
            active_connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn join_channel(&self, _guild_id: u64, _channel_id: u64) -> Result<(), VoiceError> {
        Err(VoiceError::NotImplemented {
            message: "Voice is not yet implemented. Future implementation will use:\
                \n- Songbird for Discord voice gateway\
                \n- whisper-cpp for speech-to-text\
                \n- Symphonia for audio decoding\
                \n- Rubato for audio resampling\
                \n- Hound for WAV encoding"
                .to_string(),
        })
    }

    pub async fn leave_channel(&self, _guild_id: u64) -> Result<(), VoiceError> {
        Err(VoiceError::NotImplemented {
            message: "Voice is not yet implemented.".to_string(),
        })
    }

    pub async fn is_in_voice(&self, guild_id: u64) -> bool {
        let connections = self.active_connections.read().await;
        connections.iter().any(|c| c.guild_id == guild_id)
    }

    pub async fn list_active(&self) -> Vec<(u64, u64)> {
        let connections = self.active_connections.read().await;
        connections
            .iter()
            .map(|c| (c.guild_id, c.channel_id))
            .collect()
    }
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum VoiceError {
    NotImplemented { message: String },
    AlreadyConnected,
    NotConnected,
    ConnectionFailed(String),
}

impl std::fmt::Display for VoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoiceError::NotImplemented { message } => {
                write!(f, "Voice not implemented: {}", message)
            }
            VoiceError::AlreadyConnected => write!(f, "Already in a voice channel"),
            VoiceError::NotConnected => write!(f, "Not in a voice channel"),
            VoiceError::ConnectionFailed(e) => write!(f, "Failed to connect: {}", e),
        }
    }
}

impl std::error::Error for VoiceError {}
