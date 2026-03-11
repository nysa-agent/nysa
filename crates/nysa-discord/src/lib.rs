use std::sync::Arc;

use async_trait::async_trait;
use futures::FutureExt;
use nysa_core::{
    async_trait as nysa_async_trait, Extension, ExtensionError, BackgroundTask,
    Platform, PromptContext, PromptProvider, PromptSection, ToolRegistry,
};
use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

pub mod commands;
pub mod handlers;
pub mod models;
pub mod tools;
pub mod voice;

pub use handlers::message::DiscordMessageHandler;
pub use models::{ChannelMode, DmMode, DiscordConfig, GuildConfig, ThreadState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordExtensionConfig {
    pub token: String,
    pub application_id: u64,
    pub default_mode: ChannelMode,
    pub proactive_min: i64,
    pub proactive_max: i64,
    pub dm_mode: DmMode,
    pub unauth_message: UnauthMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnauthMessage {
    pub title: String,
    pub description: String,
    pub color: i32,
}

impl Default for DiscordExtensionConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            application_id: 0,
            default_mode: ChannelMode::Thread,
            proactive_min: 60,
            proactive_max: 240,
            dm_mode: DmMode::Reactive,
            unauth_message: UnauthMessage {
                title: "Authentication Required".to_string(),
                description: "Please authenticate with Nysa using `/auth` to start chatting.".to_string(),
                color: 0xFF6B6B,
            },
        }
    }
}

#[derive(Clone)]
pub struct DiscordData {
    pub config: DiscordExtensionConfig,
    pub db: DatabaseConnection,
}

pub struct DiscordExtension {
    config: DiscordExtensionConfig,
    db: DatabaseConnection,
}

impl DiscordExtension {
    pub fn new(config: DiscordExtensionConfig, db: DatabaseConnection) -> Self {
        Self { config, db }
    }
}

#[nysa_async_trait]
impl Extension for DiscordExtension {
    fn name(&self) -> &'static str {
        "discord"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Discord platform extension with slash commands and thread management")
    }

    fn register_tools(&self, _registry: &mut ToolRegistry) {
        // Tools are registered dynamically
    }

    async fn on_start(&self) -> Result<(), ExtensionError> {
        tracing::info!("Discord extension initialized");
        Ok(())
    }

    fn background_task(&self) -> Option<BackgroundTask> {
        let token = self.config.token.clone();
        if token.is_empty() || token == "YOUR_DISCORD_BOT_TOKEN_HERE" {
            tracing::warn!("Discord token not configured. Set token in config.toml");
            return None;
        }

        let config = self.config.clone();
        let db = self.db.clone();

        Some(BackgroundTask::new("discord_gateway", async move {
            let intents = serenity::GatewayIntents::non_privileged()
                | serenity::GatewayIntents::MESSAGE_CONTENT;

            let framework = poise::Framework::builder()
                .options(poise::FrameworkOptions {
                    commands: vec![
                        commands::auth(),
                        commands::compact(),
                        commands::newthread(),
                    ],
                    prefix_options: poise::PrefixFrameworkOptions {
                        prefix: Some("!".into()),
                        // Disable mention as prefix - we handle mentions separately for conversation mode
                        mention_as_prefix: false,
                        ..Default::default()
                    },
                    event_handler: |ctx, event, framework, data| {
                        Box::pin(event_handler(ctx, event, framework, data))
                    },
                    ..Default::default()
                })
                .setup(move |_ctx, ready, _framework| {
                    let config = config.clone();
                    let db = db.clone();
                    Box::pin(async move {
                        tracing::info!("Discord bot connected as {}", ready.user.name);
                        Ok(DiscordData { config, db })
                    })
                })
                .build();

            let mut client = serenity::Client::builder(&token, intents)
                .framework(framework)
                .await
                .map_err(|e| ExtensionError::Custom(format!("Discord client error: {}", e)))?;

            client.start().await
                .map_err(|e| ExtensionError::Custom(format!("Discord gateway error: {}", e)))?;

            Ok(())
        }.boxed()))
    }

    async fn on_stop(&self) -> Result<(), ExtensionError> {
        tracing::info!("Discord extension stopped");
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, DiscordData, Box<dyn std::error::Error + Send + Sync>>,
    _data: &DiscordData,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event {
        serenity::FullEvent::Message { new_message } => {
            // Ignore bot messages
            if new_message.author.bot {
                return Ok(());
            }

            // Check if message mentions the bot
            let bot_id = ctx.cache.current_user().id;
            let is_mention = new_message.mentions.iter().any(|m| m.id == bot_id);
            
            if is_mention {
                // This is a ping - should start a conversation thread
                tracing::info!("Bot mentioned by {}: {}", new_message.author.name, new_message.content);
                
                // TODO: Implement conversation thread logic here
                // For now, just acknowledge
                let embed = serenity::CreateEmbed::new()
                    .title("Hello!")
                    .description("You mentioned me! I'll respond here shortly.")
                    .color(0x4ADE80);
                
                new_message.channel_id.send_message(
                    &ctx.http,
                    serenity::CreateMessage::new().embed(embed),
                ).await?;
            }
        }
        _ => {}
    }
    
    Ok(())
}

#[nysa_async_trait]
impl PromptProvider for DiscordExtension {
    fn provide_sections(&self, ctx: &PromptContext) -> Vec<PromptSection> {
        let mut sections = Vec::new();

        sections.push(PromptSection::new(
            "platform_rules",
            10,
            "You are interacting with a user on Discord. Discord has the following features available:\n- Slash commands (/) for user interactions\n- Threaded conversations\n- Message reactions (emoji)\n- Voice channels (in development)\n- Direct messages (DMs)",
        ));

        if ctx.platform == Platform::DiscordDm {
            sections.push(PromptSection::new(
                "dm_context",
                20,
                "You are in a direct message (DM) conversation with the user. Treat this as a more personal conversation.",
            ));
        }

        sections
    }
}
