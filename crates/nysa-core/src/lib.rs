pub mod app;
pub mod auth;
pub mod compaction;
pub mod config;
pub mod context;
pub mod database;
pub mod extension;
pub mod prompt;
pub mod tool;

pub use app::{App, AppBuilder};
pub use auth::{AuthService, AuthProvider, AuthError, PlatformProfile, generate_token, hash_token, verify_token, TokenError};
pub use compaction::{CompactionService, CompactionManager, CompactionError, CompactionResult, SearchResult};
pub use context::{Platform, PlatformDetails, UserContext, MessageContext, format_system_context};
pub use config::{Config, ConfigBuilder};
pub use prompt::{PromptSection, PromptContext, PromptProvider, PromptBuilder, PromptCondition};
pub use extension::{
    BackgroundTask, BoxFuture, Event, EventBus, Extension, ExtensionConfig, ExtensionContext,
    ExtensionDef, ExtensionError, ExtensionFactoryRegistry, ExtensionFactoryRegistryBuilder,
    ExtensionManager, ExtensionManagerBuilder, MessageReceived, MessageSource, MessageTarget,
    MessageToSend, SharedEventBus,
};
pub use tool::{
    BoxedToolHandler, PropertyType, Schema, SchemaBuilder, SchemaType, ToolDefinition,
    ToolDefinitionBuilder, ToolError, ToolExecutor, ToolHandler, ToolRegistry, ToolResult,
};
pub use tool::builtin::{DynamicToolSearchHandler, SearchDetail, ToolSearchHandler};

pub use async_trait::async_trait;
