pub mod app;
pub mod auth;
pub mod compaction;
pub mod config;
pub mod context;
pub mod database;
pub mod extension;
pub mod llm;
pub mod prompt;
pub mod tool;

pub use app::{App, AppBuilder};
pub use auth::{
    AuthError, AuthProvider, AuthService, PlatformProfile, TokenError, generate_token, hash_token,
    verify_token,
};
pub use compaction::{
    CompactionError, CompactionManager, CompactionResult, CompactionService, SearchResult,
};
pub use config::{Config, ConfigBuilder};
pub use context::{MessageContext, Platform, PlatformDetails, UserContext, format_system_context};
pub use extension::{
    BackgroundTask, BoxFuture, Event, EventBus, Extension, ExtensionConfig, ExtensionContext,
    ExtensionDef, ExtensionError, ExtensionFactoryRegistry, ExtensionFactoryRegistryBuilder,
    ExtensionManager, ExtensionManagerBuilder, MessageReceived, MessageSource, MessageTarget,
    MessageToSend, SharedEventBus, ToolsReady,
};
pub use llm::{
    Author, ConversationManager, ConversationMessage, ConversationResponse, LlmClient, LlmConfig,
    LlmError, LlmResponse, MessageHistoryService, MessageRole, ResponseMode, SystemPrompt,
    ToolCallRecord, ToolExecution, calculate_remaining_tokens, create_assistant_message,
    create_system_message, create_tool_message, create_user_message, estimate_messages_tokens,
    estimate_tokens, is_approaching_limit,
};
pub use prompt::{PromptBuilder, PromptCondition, PromptContext, PromptProvider, PromptSection};
pub use tool::builtin::{DynamicToolSearchHandler, SearchDetail, ToolSearchHandler};
pub use tool::{
    BoxedToolHandler, PropertyType, Schema, SchemaBuilder, SchemaType, ToolDefinition,
    ToolDefinitionBuilder, ToolError, ToolExecutor, ToolHandler, ToolRegistry, ToolResult,
};

pub use async_trait::async_trait;
