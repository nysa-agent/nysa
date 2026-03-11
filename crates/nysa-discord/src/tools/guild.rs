use nysa_core::{ToolDefinition, ToolHandler, ToolResult, ToolError, PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GuildInfoTool {
    serenity_client: Arc<RwLock<Option<serenity::Client>>>,
}

impl GuildInfoTool {
    pub fn new() -> Self {
        Self {
            serenity_client: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_client(mut self, client: serenity::Client) -> Self {
        self.serenity_client = Arc::new(RwLock::new(Some(client)));
        self
    }
}

#[async_trait]
impl ToolHandler for GuildInfoTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let guild_id = args.get("guild_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("guild_id is required".to_string()))?;

        let action = args.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get_guild_info");

        let lock = self.serenity_client.read().await;
        let client = lock.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Discord client not initialized".to_string())
        })?;

        let guild = serenity::GuildId(guild_id).to_partial_guild(&client.http).await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get guild: {}", e)))?;

        match action {
            "get_guild_info" => {
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
            }
            "get_roles" => {
                let roles = guild.roles(&client.http).await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get roles: {}", e)))?;
                
                let roles_json: Vec<serde_json::Value> = roles.iter().map(|(id, role)| {
                    serde_json::json!({
                        "id": id.get(),
                        "name": role.name,
                        "color": role.colour.0,
                        "position": role.position,
                        "permissions": role.permissions.0.bits(),
                    })
                }).collect();
                
                Ok(ToolResult::success(serde_json::to_string(&roles_json).unwrap_or_default()))
            }
            "get_channels" => {
                let channels = guild.channels(&client.http).await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get channels: {}", e)))?;
                
                let channels_json: Vec<serde_json::Value> = channels.iter().map(|(id, channel)| {
                    serde_json::json!({
                        "id": id.get(),
                        "name": channel.name(),
                        "kind": format!("{:?}", channel.kind()),
                    })
                }).collect();
                
                Ok(ToolResult::success(serde_json::to_string(&channels_json).unwrap_or_default()))
            }
            _ => Err(ToolError::InvalidArguments(format!("Unknown action: {}", action))),
        }
    }
}

pub fn register(registry: &mut nysa_core::ToolRegistry) {
    let tool = ToolDefinition::builder()
        .name("guild_info")
        .description("Get information about a Discord server (guild), including roles and channels")
        .parameters(
            SchemaBuilder::object()
                .property("guild_id", PropertyType::string().description("The guild/server ID"))
                .property("action", PropertyType::string()
                    .description("Action: get_guild_info, get_roles, get_channels")
                    .enum_values(vec!["get_guild_info", "get_roles", "get_channels"]))
                .required("guild_id")
                .build(),
        )
        .category("discord")
        .build()
        .expect("Failed to build guild_info tool");

    registry.register(tool, GuildInfoTool::new());
}
