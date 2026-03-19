use nysa_core::{ToolError, ToolHandler, ToolResult, async_trait};
use poise::serenity_prelude as serenity;
use serde_json::Value;

use super::DiscordToolContext;

pub struct MessageHistoryTool {
    ctx: DiscordToolContext,
}

impl MessageHistoryTool {
    pub fn new(ctx: DiscordToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolHandler for MessageHistoryTool {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let channel_id = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("channel_id is required".to_string()))?;

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|l| l.min(100) as u8)
            .unwrap_or(50);

        let before_id = args
            .get("before_message_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .map(serenity::MessageId::new);

        let channel = serenity::ChannelId::new(channel_id);

        let mut builder = serenity::GetMessages::new().limit(limit);
        if let Some(before) = before_id {
            builder = builder.before(before);
        }

        match channel.messages(&self.ctx.http, builder).await {
            Ok(messages) => {
                let history: Vec<serde_json::Value> = messages
                    .iter()
                    .map(|msg| {
                        serde_json::json!({
                            "id": msg.id.get(),
                            "author_id": msg.author.id.get(),
                            "author_name": msg.author.name,
                            "content": msg.content,
                            "timestamp": msg.timestamp.timestamp(),
                            "edited": msg.edited_timestamp.is_some(),
                            "attachments": msg.attachments.len(),
                            "embeds": msg.embeds.len(),
                        })
                    })
                    .collect();

                Ok(ToolResult::success(
                    serde_json::to_string(&history).unwrap_or_default(),
                ))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to get message history: {}",
                e
            ))),
        }
    }
}
