use nysa_core::{ToolDefinition, ToolHandler, ToolResult, ToolError, PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ChannelManagementTool {
    serenity_client: Arc<RwLock<Option<serenity::Client>>>,
}

impl ChannelManagementTool {
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
impl ToolHandler for ChannelManagementTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let action = args.get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("action is required".to_string()))?;

        let channel_id = args.get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        let lock = self.serenity_client.read().await;
        let client = lock.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Discord client not initialized".to_string())
        })?;

        let channel = serenity::ChannelId(channel_id.ok_or_else(|| 
            ToolError::InvalidArguments("channel_id is required".to_string())
        )?);

        match action {
            "create_thread" => {
                let name = args.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("New Thread");
                
                let thread = channel.create_thread(serenity::CreateThread::new(name)
                    .kind(serenity::ChannelType::PublicThread)
                    .invitable(false),
                    &client.http
                ).await.map_err(|e| ToolError::ExecutionFailed(format!("Failed to create thread: {}", e)))?;

                Ok(ToolResult::success(serde_json::json!({
                    "thread_id": thread.id.get(),
                    "name": name,
                }).to_string()))
            }
            "edit_message" => {
                let message_id = args.get("message_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| ToolError::InvalidArguments("message_id is required".to_string()))?;
                
                let content = args.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("content is required".to_string()))?;

                channel.edit_message(&client.http, message_id, |m| m.content(content))
                    .await.map_err(|e| ToolError::ExecutionFailed(format!("Failed to edit message: {}", e)))?;

                Ok(ToolResult::success(format!("Edited message {}", message_id)))
            }
            "pin_message" => {
                let message_id = args.get("message_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| ToolError::InvalidArguments("message_id is required".to_string()))?;

                channel.pin(&client.http, message_id)
                    .await.map_err(|e| ToolError::ExecutionFailed(format!("Failed to pin message: {}", e)))?;

                Ok(ToolResult::success(format!("Pinned message {}", message_id)))
            }
            "unpin_message" => {
                let message_id = args.get("message_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| ToolError::InvalidArguments("message_id is required".to_string()))?;

                channel.unpin(&client.http, message_id)
                    .await.map_err(|e| ToolError::ExecutionFailed(format!("Failed to unpin message: {}", e)))?;

                Ok(ToolResult::success(format!("Unpinned message {}", message_id)))
            }
            "get_channel_info" => {
                let channel_info = channel.to_channel(&client.http).await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get channel: {}", e)))?;

                let info = match channel_info {
                    serenity::Channel::Guild(c) => serde_json::json!({
                        "id": c.id.get(),
                        "name": c.name,
                        "topic": c.topic,
                        "kind": format!("{:?}", c.kind),
                        "position": c.position,
                    }),
                    serenity::Channel::Private(c) => serde_json::json!({
                        "id": c.id.get(),
                        "kind": "private",
                    }),
                    _ => serde_json::json!({"error": "Unknown channel type"}),
                };

                Ok(ToolResult::success(info.to_string()))
            }
            _ => Err(ToolError::InvalidArguments(format!("Unknown action: {}", action))),
        }
    }
}

pub fn register(registry: &mut nysa_core::ToolRegistry) {
    let tool = ToolDefinition::builder()
        .name("channel_management")
        .description("Manage Discord channels and threads (create threads, edit/pin messages, get channel info)")
        .parameters(
            SchemaBuilder::object()
                .property("action", PropertyType::string()
                    .description("Action to perform: create_thread, edit_message, pin_message, unpin_message, get_channel_info")
                    .enum_values(vec!["create_thread", "edit_message", "pin_message", "unpin_message", "get_channel_info"]))
                .property("channel_id", PropertyType::string().description("The channel ID"))
                .property("message_id", PropertyType::string().description("The message ID (for edit/pin/unpin)"))
                .property("name", PropertyType::string().description("Name (for create_thread)"))
                .property("content", PropertyType::string().description("Content (for edit_message)"))
                .required("action")
                .build(),
        )
        .category("discord")
        .build()
        .expect("Failed to build channel_management tool");

    registry.register(tool, ChannelManagementTool::new());
}
