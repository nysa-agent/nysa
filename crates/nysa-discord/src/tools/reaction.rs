use nysa_core::{ToolDefinition, ToolHandler, ToolResult, ToolError, PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MessageReactionTool {
    serenity_client: Arc<RwLock<Option<serenity::Client>>>,
}

impl MessageReactionTool {
    pub fn new() -> Self {
        Self {
            serenity_client: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_client(mut self, client: serenity::Client) -> Self {
        self.serenity_client = Arc::new(RwLock::new(Some(client)));
        self
    }

    pub async fn set_client(&self, client: serenity::Client) {
        let mut lock = self.serenity_client.write().await;
        *lock = Some(client);
    }
}

#[async_trait]
impl ToolHandler for MessageReactionTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let channel_id = args.get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("channel_id is required".to_string()))?;

        let message_id = args.get("message_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("message_id is required".to_string()))?;

        let emoji = args.get("emoji")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("emoji is required".to_string()))?;

        let lock = self.serenity_client.read().await;
        let client = lock.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Discord client not initialized".to_string())
        })?;

        let channel = serenity::ChannelId(channel_id);
        
        let parsed_emoji = if emoji.starts_with('<') && emoji.ends_with('>') {
            serenity::ReactionType::Custom {
                id: serenity::EmojiId(emoji[emoji.find(':').map(|i| i + 1).unwrap_or(0)..].parse().unwrap_or(0)),
                name: None,
                animated: false,
            }
        } else {
            serenity::ReactionType::Unicode(emoji.to_string())
        };

        match channel.react(&client.http, message_id, parsed_emoji).await {
            Ok(_) => Ok(ToolResult::success(format!("Added reaction {} to message {}", emoji, message_id))),
            Err(e) => Ok(ToolResult::error(format!("Failed to add reaction: {}", e))),
        }
    }
}

pub fn register(registry: &mut nysa_core::ToolRegistry) {
    let tool = ToolDefinition::builder()
        .name("message_reaction")
        .description("Add an emoji reaction to a message")
        .parameters(
            SchemaBuilder::object()
                .property("channel_id", PropertyType::string().description("The channel ID where the message is"))
                .property("message_id", PropertyType::string().description("The message ID to react to"))
                .property("emoji", PropertyType::string().description("The emoji to add (unicode or custom)"))
                .required("channel_id")
                .required("message_id")
                .required("emoji")
                .build(),
        )
        .category("discord")
        .build()
        .expect("Failed to build message_reaction tool");

    registry.register(tool, MessageReactionTool::new());
}
