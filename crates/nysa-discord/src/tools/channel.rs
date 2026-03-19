use nysa_core::{ToolError, ToolHandler, ToolResult, async_trait};
use poise::serenity_prelude as serenity;
use serde_json::Value;

use super::DiscordToolContext;

pub struct ChannelManagementTool {
    ctx: DiscordToolContext,
}

impl ChannelManagementTool {
    pub fn new(ctx: DiscordToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolHandler for ChannelManagementTool {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let channel_id = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("channel_id is required".to_string()))?;

        let channel = serenity::ChannelId::new(channel_id);

        // Check if this is a thread creation request
        if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
            // Create thread from message
            let message_id = args
                .get("message_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::MessageId::new)
                .ok_or_else(|| {
                    ToolError::InvalidArguments(
                        "message_id is required for thread creation".to_string(),
                    )
                })?;

            match channel
                .create_thread_from_message(
                    &self.ctx.http,
                    message_id,
                    serenity::CreateThread::new(name)
                        .kind(serenity::ChannelType::PublicThread)
                        .invitable(false),
                )
                .await
            {
                Ok(thread) => Ok(ToolResult::success(format!(
                    "Created thread '{}' with ID {}",
                    name,
                    thread.id.get()
                ))),
                Err(e) => Ok(ToolResult::error(format!("Failed to create thread: {}", e))),
            }
        } else if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
            // Edit message
            let message_id = args
                .get("message_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::MessageId::new)
                .ok_or_else(|| {
                    ToolError::InvalidArguments("message_id is required for editing".to_string())
                })?;

            match channel
                .edit_message(
                    &self.ctx.http,
                    message_id,
                    serenity::EditMessage::new().content(content),
                )
                .await
            {
                Ok(_) => Ok(ToolResult::success(
                    "Message edited successfully".to_string(),
                )),
                Err(e) => Ok(ToolResult::error(format!("Failed to edit message: {}", e))),
            }
        } else {
            // Pin/Unpin operation
            let message_id = args
                .get("message_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::MessageId::new)
                .ok_or_else(|| ToolError::InvalidArguments("message_id is required".to_string()))?;

            match channel.pin(&self.ctx.http, message_id).await {
                Ok(_) => Ok(ToolResult::success(
                    "Message pinned successfully".to_string(),
                )),
                Err(e) => Ok(ToolResult::error(format!("Failed to pin message: {}", e))),
            }
        }
    }
}
