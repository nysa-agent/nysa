use async_openai::types::{ChatCompletionMessageToolCall, CompletionUsage, FinishReason};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub enum ResponseMode {
    Batch,
    Stream,
}

impl Default for ResponseMode {
    fn default() -> Self {
        ResponseMode::Batch
    }
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub max_context_tokens: usize,
    pub compaction_enabled: bool,
    pub compaction_threshold: f32,
    pub max_tool_iterations: u8,
    pub default_mode: ResponseMode,
    pub system_prompt_override: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 120_000,
            compaction_enabled: true,
            compaction_threshold: 0.75,
            max_tool_iterations: 10,
            default_mode: ResponseMode::Batch,
            system_prompt_override: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Context too long: {0} tokens exceeds limit of {1}")]
    ContextTooLong(usize, usize),

    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Max iterations reached: {0}")]
    MaxIterationsReached(u8),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("No response from LLM")]
    NoResponse,

    #[error("Invalid tool arguments: {0}")]
    InvalidToolArguments(String),

    #[error("Streaming error: {0}")]
    StreamingError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<sea_orm::DbErr> for LlmError {
    fn from(err: sea_orm::DbErr) -> Self {
        LlmError::DatabaseError(err.to_string())
    }
}

impl From<async_openai::error::OpenAIError> for LlmError {
    fn from(err: async_openai::error::OpenAIError) -> Self {
        LlmError::ApiError(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ChatCompletionMessageToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Option<CompletionUsage>,
}

impl LlmResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct StreamDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConversationResponse {
    pub content: String,
    pub tool_calls_made: Vec<ToolExecution>,
    pub finish_reason: FinishReason,
    pub tokens_used: Option<CompletionUsage>,
}

#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub author_name: Option<String>,
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    pub tool_call_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCallRecord {
    pub fn parse_arguments(&self) -> Result<serde_json::Value, LlmError> {
        serde_json::from_str(&self.arguments).map_err(|e| {
            LlmError::InvalidToolArguments(format!(
                "Failed to parse tool arguments for {}: {}",
                self.name, e
            ))
        })
    }
}

pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub name: String,
    pub result: String,
}

pub struct Author {
    pub id: Uuid,
    pub name: String,
    pub platform_id: String,
}

impl Author {
    pub fn new(id: Uuid, name: String, platform_id: String) -> Self {
        Self {
            id,
            name,
            platform_id,
        }
    }
}
