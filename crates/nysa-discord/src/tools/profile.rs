use nysa_core::{ToolDefinition, ToolHandler, ToolResult, ToolError, PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GetUserProfileTool {
    serenity_client: Arc<RwLock<Option<serenity::Client>>>,
}

impl GetUserProfileTool {
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
impl ToolHandler for GetUserProfileTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let user_id = args.get("user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("user_id is required".to_string()))?;

        let lock = self.serenity_client.read().await;
        let client = lock.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Discord client not initialized".to_string())
        })?;

        let user = serenity::UserId(user_id).to_user(&client.http).await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get user: {}", e)))?;

        let profile = serde_json::json!({
            "id": user.id.get(),
            "username": user.name,
            "display_name": user.display_name(),
            "avatar": user.avatar_url(),
            "banner": user.banner_url(),
            "discriminator": user.discriminator,
            "is_bot": user.bot,
            "created_at": user.created_at().timestamp(),
        });

        Ok(ToolResult::success(profile.to_string()))
    }
}

pub fn register(registry: &mut nysa_core::ToolRegistry) {
    let tool = ToolDefinition::builder()
        .name("get_user_profile")
        .description("Get information about a Discord user")
        .parameters(
            SchemaBuilder::object()
                .property("user_id", PropertyType::string().description("The user's ID"))
                .required("user_id")
                .build(),
        )
        .category("discord")
        .build()
        .expect("Failed to build get_user_profile tool");

    registry.register(tool, GetUserProfileTool::new());
}
