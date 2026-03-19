use nysa_core::{ToolError, ToolHandler, ToolResult, async_trait};
use poise::serenity_prelude as serenity;
use serde_json::Value;

use super::DiscordToolContext;

pub struct GuildInfoTool {
    ctx: DiscordToolContext,
}

impl GuildInfoTool {
    pub fn new(ctx: DiscordToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolHandler for GuildInfoTool {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Check if we have a guild_id (for guild info) or channel_id (for channel info)
        if let Some(guild_id) = args
            .get("guild_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
        {
            let guild = serenity::GuildId::new(guild_id)
                .to_partial_guild(&self.ctx.http)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get guild: {}", e)))?;

            let info = serde_json::json!({
                "id": guild.id.get(),
                "name": guild.name,
                "icon": guild.icon_url(),
                "banner": guild.banner_url(),
                "splash": guild.splash_url(),
                "member_count": guild.approximate_member_count,
                "premium_tier": guild.premium_tier,
                "description": guild.description,
                "features": guild.features,
            });

            Ok(ToolResult::success(info.to_string()))
        } else if let Some(channel_id) = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
        {
            let channel = serenity::ChannelId::new(channel_id)
                .to_channel(&self.ctx.http)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get channel: {}", e)))?;

            let info = match channel {
                serenity::Channel::Guild(c) => serde_json::json!({
                    "id": c.id.get(),
                    "name": c.name,
                    "type": format!("{:?}", c.kind),
                    "topic": c.topic,
                    "position": c.position,
                    "nsfw": c.nsfw,
                }),
                serenity::Channel::Private(c) => serde_json::json!({
                    "id": c.id.get(),
                    "type": "DM",
                    "recipient": c.recipient.name.clone(),
                }),
                _ => serde_json::json!({"error": "Unknown channel type"}),
            };

            Ok(ToolResult::success(info.to_string()))
        } else {
            Err(ToolError::InvalidArguments(
                "Either guild_id or channel_id is required".to_string(),
            ))
        }
    }
}
