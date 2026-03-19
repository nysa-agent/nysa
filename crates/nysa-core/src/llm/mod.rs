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
pub use client::{
    LlmClient, 
    create_user_message,
    create_system_message, 
    create_assistant_message,
    create_tool_message,
};
pub use crate::config::NysaOpenAiConfig;
pub use conversation::ConversationManager;
pub use history::MessageHistoryService;
pub use prompt::{SystemPrompt, load_system_prompt};
pub use tokenizer::{
    estimate_tokens,
    estimate_messages_tokens,
    is_approaching_limit,
    calculate_remaining_tokens,
};
pub use types::{
    Author,
    ConversationMessage,
    ConversationResponse,
    LlmConfig,
    LlmError,
    LlmResponse,
    MessageRole,
    ResponseMode,
    StreamDelta,
    ToolCallRecord,
    ToolExecution,
    ToolResultMessage,
};
