use nysa_core::{ToolHandler, ToolResult, ToolError, async_trait};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::Mentionable;
use serde_json::Value;

use super::DiscordToolContext;

pub struct GetUserProfileTool {
    ctx: DiscordToolContext,
}

impl GetUserProfileTool {
    pub fn new(ctx: DiscordToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolHandler for GetUserProfileTool {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let user_id = args.get("user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("user_id is required".to_string()))?;

        let user = serenity::UserId::new(user_id)
            .to_user(&self.ctx.http)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get user: {}", e)))?;

        let profile = serde_json::json!({
            "id": user.id.get(),
            "username": user.name,
            "display_name": user.display_name(),
            "avatar": user.avatar_url(),
            "banner": user.banner_url(),
            "discriminator": user.discriminator.map(|d| d.get()),
            "is_bot": user.bot,
            "created_at": user.created_at().timestamp(),
            "mention": user.mention().to_string(),
        });

        Ok(ToolResult::success(profile.to_string()))
    }
}
