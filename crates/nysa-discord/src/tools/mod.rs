use nysa_core::{ToolRegistry, ToolDefinition, ToolHandler, ToolResult, ToolError};
use nysa_core::{PropertyType, SchemaBuilder, async_trait};
use poise::serenity_prelude as serenity;
use serde_json::Value;
use std::sync::Arc;

mod reaction;
mod profile;
mod channel;
mod history;
mod guild;
mod voice;

pub use reaction::MessageReactionTool;
pub use profile::GetUserProfileTool;
pub use channel::ChannelManagementTool;
pub use history::MessageHistoryTool;
pub use guild::GuildInfoTool;
pub use voice::VoiceChannelTool;

/// Context passed to Discord tools
#[derive(Clone)]
pub struct DiscordToolContext {
    pub http: Arc<serenity::Http>,
    pub cache: Arc<serenity::Cache>,
    pub bot_id: u64,
}

impl DiscordToolContext {
    pub fn new(http: Arc<serenity::Http>, cache: Arc<serenity::Cache>, bot_id: u64) -> Self {
        Self { http, cache, bot_id }
    }
}

/// Register all Discord tools with the registry
pub fn register_all(registry: &mut ToolRegistry, ctx: DiscordToolContext) {
    // Message reaction tool
    registry.register(
        ToolDefinition::builder()
            .name("message_reaction")
            .description("Add an emoji reaction to a message")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID where the message is"))
                    .property("message_id", PropertyType::string().description("The message ID to react to"))
                    .property("emoji", PropertyType::string().description("The emoji to add (unicode or custom)"))
                    .required("channel_id")
                    .required("message_id")
                    .required("emoji")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build message_reaction tool"),
        MessageReactionTool::new(ctx.clone()),
    );

    // Get user profile tool
    registry.register(
        ToolDefinition::builder()
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
            .expect("Failed to build get_user_profile tool"),
        GetUserProfileTool::new(ctx.clone()),
    );

    // Channel management tool
    registry.register(
        ToolDefinition::builder()
            .name("create_thread")
            .description("Create a new thread from a message")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID where the message is"))
                    .property("message_id", PropertyType::string().description("The message ID to create thread from"))
                    .property("name", PropertyType::string().description("The name of the thread"))
                    .required("channel_id")
                    .required("message_id")
                    .required("name")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build create_thread tool"),
        ChannelManagementTool::new(ctx.clone()),
    );

    // Edit message tool
    registry.register(
        ToolDefinition::builder()
            .name("edit_message")
            .description("Edit one of the bot's messages")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID"))
                    .property("message_id", PropertyType::string().description("The message ID to edit"))
                    .property("content", PropertyType::string().description("The new content"))
                    .required("channel_id")
                    .required("message_id")
                    .required("content")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build edit_message tool"),
        ChannelManagementTool::new(ctx.clone()),
    );

    // Pin message tool
    registry.register(
        ToolDefinition::builder()
            .name("pin_message")
            .description("Pin a message in a channel")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID"))
                    .property("message_id", PropertyType::string().description("The message ID to pin"))
                    .required("channel_id")
                    .required("message_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build pin_message tool"),
        ChannelManagementTool::new(ctx.clone()),
    );

    // Unpin message tool
    registry.register(
        ToolDefinition::builder()
            .name("unpin_message")
            .description("Unpin a message in a channel")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID"))
                    .property("message_id", PropertyType::string().description("The message ID to unpin"))
                    .required("channel_id")
                    .required("message_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build unpin_message tool"),
        ChannelManagementTool::new(ctx.clone()),
    );

    // Message history tool
    registry.register(
        ToolDefinition::builder()
            .name("search_history")
            .description("Get message history from a channel")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID"))
                    .property("limit", PropertyType::integer().description("Number of messages to retrieve (max 100)").minimum(1).maximum(100))
                    .property("before_message_id", PropertyType::string().description("Optional message ID to get messages before"))
                    .required("channel_id")
                    .required("limit")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build search_history tool"),
        MessageHistoryTool::new(ctx.clone()),
    );

    // Guild info tool
    registry.register(
        ToolDefinition::builder()
            .name("get_guild_info")
            .description("Get information about a Discord server (guild)")
            .parameters(
                SchemaBuilder::object()
                    .property("guild_id", PropertyType::string().description("The guild ID"))
                    .required("guild_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build get_guild_info tool"),
        GuildInfoTool::new(ctx.clone()),
    );

    // Channel info tool
    registry.register(
        ToolDefinition::builder()
            .name("get_channel_info")
            .description("Get information about a Discord channel")
            .parameters(
                SchemaBuilder::object()
                    .property("channel_id", PropertyType::string().description("The channel ID"))
                    .required("channel_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build get_channel_info tool"),
        GuildInfoTool::new(ctx.clone()),
    );

    // Voice channel tools (placeholders)
    registry.register(
        ToolDefinition::builder()
            .name("join_voice")
            .description("Join a voice channel (placeholder - voice not yet implemented)")
            .parameters(
                SchemaBuilder::object()
                    .property("guild_id", PropertyType::string().description("The guild ID"))
                    .property("channel_id", PropertyType::string().description("The voice channel ID"))
                    .required("guild_id")
                    .required("channel_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build join_voice tool"),
        VoiceChannelTool::new(),
    );

    registry.register(
        ToolDefinition::builder()
            .name("leave_voice")
            .description("Leave a voice channel (placeholder - voice not yet implemented)")
            .parameters(
                SchemaBuilder::object()
                    .property("guild_id", PropertyType::string().description("The guild ID"))
                    .required("guild_id")
                    .build(),
            )
            .category("discord")
            .build()
            .expect("Failed to build leave_voice tool"),
        VoiceChannelTool::new(),
    );
}

/// Helper trait for tools that need Discord context
pub trait DiscordTool: ToolHandler {
    fn with_context(ctx: DiscordToolContext) -> Self where Self: Sized;
}

/// Parse a channel ID from string
pub fn parse_channel_id(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}

/// Parse a message ID from string
pub fn parse_message_id(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}

/// Parse a user ID from string
pub fn parse_user_id(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}

/// Parse a guild ID from string
pub fn parse_guild_id(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}
