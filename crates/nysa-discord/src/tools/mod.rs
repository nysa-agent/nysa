use uuid::Uuid;

pub fn register_all(_registry: &mut nysa_core::ToolRegistry) {
    // Placeholder - tools will be registered after proper integration
    // Available tools planned:
    // - message_reaction: Add emoji reaction to a message
    // - get_user_profile: Get Discord user info
    // - channel_management: Create threads, edit/pin messages
    // - message_history: Get channel message history
    // - guild_info: Get server info, roles, channels
    // - voice_channel: Voice management (placeholder - not implemented)
}

pub struct MessageReactionTool;
pub struct GetUserProfileTool;
pub struct ChannelManagementTool;
pub struct MessageHistoryTool;
pub struct GuildInfoTool;
pub struct VoiceChannelTool;
