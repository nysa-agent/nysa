use nysa_core::{ToolHandler, ToolResult, ToolError, async_trait};
use poise::serenity_prelude as serenity;
use serde_json::Value;

use super::DiscordToolContext;

pub struct MessageReactionTool {
    ctx: DiscordToolContext,
}

impl MessageReactionTool {
    pub fn new(ctx: DiscordToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolHandler for MessageReactionTool {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
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

        let channel = serenity::ChannelId::new(channel_id);
        let message = serenity::MessageId::new(message_id);
        
        let parsed_emoji = if emoji.starts_with('<') && emoji.ends_with('>') {
            // Custom emoji format: <:name:id>
            let parts: Vec<&str> = emoji.trim_matches(|c| c == '<' || c == '>').split(':').collect();
            if parts.len() >= 2 {
                if let Ok(emoji_id) = parts.last().unwrap().parse::<u64>() {
                    serenity::ReactionType::Custom {
                        id: serenity::EmojiId::new(emoji_id),
                        name: Some(parts[0].to_string()),
                        animated: false,
                    }
                } else {
                    serenity::ReactionType::Unicode(emoji.to_string())
                }
            } else {
                serenity::ReactionType::Unicode(emoji.to_string())
            }
        } else {
            serenity::ReactionType::Unicode(emoji.to_string())
        };

        // Use channel's create_reaction method
        match channel.create_reaction(&self.ctx.http, message, parsed_emoji).await {
            Ok(_) => Ok(ToolResult::success(format!("Added reaction {} to message {}", emoji, message_id))),
            Err(e) => Ok(ToolResult::error(format!("Failed to add reaction: {}", e))),
        }
    }
}
