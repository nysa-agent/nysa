use nysa_core::{ToolDefinition, ToolHandler, ToolResult, ToolError, PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MessageHistoryTool {
    serenity_client: Arc<RwLock<Option<serenity::Client>>>,
}

impl MessageHistoryTool {
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
impl ToolHandler for MessageHistoryTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let channel_id = args.get("channel_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| ToolError::InvalidArguments("channel_id is required".to_string()))?;

        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let lock = self.serenity_client.read().await;
        let client = lock.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Discord client not initialized".to_string())
        })?;

        let channel = serenity::ChannelId(channel_id);
        
        let messages = channel.messages(&client.http, |m| m.limit(limit as u64))
            .await.map_err(|e| ToolError::ExecutionFailed(format!("Failed to get messages: {}", e)))?;

        let history: Vec<serde_json::Value> = messages.iter().map(|msg| {
            serde_json::json!({
                "id": msg.id.get(),
                "author": msg.author.name,
                "content": msg.content,
                "timestamp": msg.timestamp.timestamp(),
            })
        }).collect();

        Ok(ToolResult::success(serde_json::to_string(&history).unwrap_or_default()))
    }
}

pub fn register(registry: &mut nysa_core::ToolRegistry) {
    let tool = ToolDefinition::builder()
        .name("message_history")
        .description("Get message history from a channel")
        .parameters(
            SchemaBuilder::object()
                .property("channel_id", PropertyType::string().description("The channel ID"))
                .property("limit", PropertyType::integer().description("Number of messages to retrieve (default 10)"))
                .required("channel_id")
                .build(),
        )
        .category("discord")
        .build()
        .expect("Failed to build message_history tool");

    registry.register(tool, MessageHistoryTool::new());
}
