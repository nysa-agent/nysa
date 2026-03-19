//! LLM Integration Module
//!
//! Provides comprehensive LLM capabilities for Nysa:
//! - Client for API communication (OpenAI-compatible, including OpenRouter)
//! - Conversation management with tool execution
//! - Message history persistence
//! - Context compaction
//! - Streaming and batch responses

pub mod client;
pub mod conversation;
pub mod history;
pub mod prompt;
pub mod tokenizer;
pub mod types;

// Re-export main types
pub use crate::config::NysaOpenAiConfig;
pub use client::{
    LlmClient, create_assistant_message, create_system_message, create_tool_message,
    create_user_message,
};
pub use conversation::ConversationManager;
pub use history::MessageHistoryService;
pub use prompt::{SystemPrompt, load_system_prompt};
pub use tokenizer::{
    calculate_remaining_tokens, estimate_messages_tokens, estimate_tokens, is_approaching_limit,
};
pub use types::{
    Author, ConversationMessage, ConversationResponse, LlmConfig, LlmError, LlmResponse,
    MessageRole, ResponseMode, StreamDelta, ToolCallRecord, ToolExecution, ToolResultMessage,
};
