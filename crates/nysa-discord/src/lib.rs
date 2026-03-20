use std::sync::Arc;

use futures::FutureExt;
use nysa_core::{
    BackgroundTask, ConversationManager, Extension, ExtensionContext, ExtensionError, Platform,
    PromptContext, PromptProvider, PromptSection, ToolRegistry, ToolsReady,
    async_trait as nysa_async_trait,
};
use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

pub mod commands;
pub mod handlers;
pub mod models;
pub mod tools;
pub mod voice;

pub use handlers::auth::{AuthMiddleware, AuthenticatedUser};
pub use handlers::dm::DmHandler;
pub use handlers::evaluate::EvaluateAllHandler;
pub use handlers::message::DiscordMessageHandler;
pub use handlers::proactive::ProactiveManager;
pub use handlers::thread::ThreadManager;
pub use models::{ChannelMode, DiscordConfig, DmMode, GuildConfig, ThreadState};

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
                description: "Please authenticate with Nysa using `/auth` to start chatting."
                    .to_string(),
                color: 0xFF6B6B,
            },
        }
    }
}

#[derive(Clone)]
pub struct DiscordData {
    pub config: DiscordExtensionConfig,
    pub db: DatabaseConnection,
    pub message_handler: DiscordMessageHandler,
    pub auth_middleware: AuthMiddleware,
    pub thread_manager: ThreadManager,
    pub dm_handler: DmHandler,
    pub proactive_manager: ProactiveManager,
    pub evaluate_handler: EvaluateAllHandler,
    pub conversation_manager: Option<Arc<ConversationManager>>,
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
        Some(
            "Discord platform extension with slash commands, thread management, and full authentication",
        )
    }

    fn prompt_provider(&self) -> Option<&dyn PromptProvider> {
        Some(self)
    }

    fn register_tools(&self, _registry: &mut ToolRegistry) {
        // Tools will be registered when we have access to the Discord context
        // This happens in the background task when the client is initialized
        tracing::debug!("Tools will be registered on client initialization");
    }

    async fn on_start(&self) -> Result<(), ExtensionError> {
        tracing::info!("Discord extension initialized");
        Ok(())
    }

    fn background_task(&self, ctx: &ExtensionContext) -> Option<BackgroundTask> {
        let token = self.config.token.clone();
        if token.is_empty() || token == "YOUR_DISCORD_BOT_TOKEN_HERE" {
            tracing::warn!("Discord token not configured. Set token in config.toml");
            return None;
        }

        let config = self.config.clone();
        let db = self.db.clone();
        let conversation_manager = ctx.conversation().cloned();
        let tool_registry = ctx.tool_registry.clone();
        let event_bus = ctx.event_bus.clone();

        Some(BackgroundTask::new(
            "discord_gateway",
            async move {
                // Initialize handlers
                let discord_config = DiscordConfig {
                    token: config.token.clone(),
                    application_id: config.application_id,
                    default_mode: config.default_mode,
                    proactive_min: config.proactive_min,
                    proactive_max: config.proactive_max,
                    dm_mode: config.dm_mode,
                    unauth_message: crate::models::UnauthMessageTemplate {
                        title: config.unauth_message.title.clone(),
                        description: config.unauth_message.description.clone(),
                        color: config.unauth_message.color,
                    },
                };

                let message_handler = DiscordMessageHandler::new(discord_config);
                let auth_middleware = AuthMiddleware::new(db.clone());
                let thread_manager = ThreadManager::new();
                let dm_handler = DmHandler::new(config.dm_mode);
                let proactive_manager =
                    ProactiveManager::new(config.proactive_min, config.proactive_max);
                let evaluate_handler = EvaluateAllHandler::new();

                let intents = serenity::GatewayIntents::non_privileged()
                    | serenity::GatewayIntents::MESSAGE_CONTENT
                    | serenity::GatewayIntents::GUILDS
                    | serenity::GatewayIntents::GUILD_MEMBERS
                    | serenity::GatewayIntents::GUILD_MESSAGES
                    | serenity::GatewayIntents::DIRECT_MESSAGES;

                let framework = poise::Framework::builder()
                    .options(poise::FrameworkOptions {
                        commands: vec![
                            commands::auth(),
                            commands::generate_link(),
                            commands::compact(),
                            commands::newthread(),
                            commands::help(),
                            commands::settings(),
                        ],
                        prefix_options: poise::PrefixFrameworkOptions {
                            prefix: Some("!".into()),
                            mention_as_prefix: false,
                            ..Default::default()
                        },
                        event_handler: |ctx, event, framework, data| {
                            Box::pin(event_handler(ctx, event, framework, data))
                        },
                        ..Default::default()
                    })
                    .setup(move |_ctx, ready, _framework| {
                        tracing::info!("Discord bot connected as {}", ready.user.name);

                        let data = DiscordData {
                            config: config.clone(),
                            db: db.clone(),
                            message_handler: message_handler.clone(),
                            auth_middleware: auth_middleware.clone(),
                            thread_manager: thread_manager.clone(),
                            dm_handler: dm_handler.clone(),
                            proactive_manager: proactive_manager.clone(),
                            evaluate_handler: evaluate_handler.clone(),
                            conversation_manager,
                        };

                        Box::pin(async move { Ok(data) })
                    })
                    .build();

                let mut client = serenity::Client::builder(&token, intents)
                    .framework(framework)
                    .await
                    .map_err(|e| ExtensionError::Custom(format!("Discord client error: {}", e)))?;

                // Get references to http and cache for tool context
                // These are accessed through the client's fields in serenity 0.12
                let http = Arc::clone(&client.http);
                let cache = Arc::clone(&client.cache);
                let bot_id = client.cache.current_user().id.get();

                let tool_ctx = tools::DiscordToolContext::new(http, cache, bot_id);

                // Register Discord tools once we have the client context
                {
                    let mut registry = tool_registry.write().await;
                    tools::register_all(&mut registry, tool_ctx);
                    let tool_count = registry.all().len();
                    tracing::info!("Registered Discord tools");

                    event_bus.publish(ToolsReady {
                        extension_name: "discord".to_string(),
                        tool_count,
                    });
                }

                client
                    .start()
                    .await
                    .map_err(|e| ExtensionError::Custom(format!("Discord gateway error: {}", e)))?;

                Ok(())
            }
            .boxed(),
        ))
    }

    async fn on_stop(&self) -> Result<(), ExtensionError> {
        tracing::info!("Discord extension stopped");
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

async fn process_message_with_llm(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &DiscordData,
    thread_id: uuid::Uuid,
) -> Result<(String, u64), Box<dyn std::error::Error + Send + Sync>> {
    let conversation_manager = match &data.conversation_manager {
        Some(manager) => manager,
        None => {
            return Err("AI not configured".into());
        }
    };

    message.channel_id.broadcast_typing(&ctx.http).await?;

    let (platform, platform_label) = if message.guild_id.is_some() {
        (nysa_core::Platform::DiscordGuild, "discord_guild")
    } else {
        (nysa_core::Platform::DiscordDm, "discord_dm")
    };

    let mut msg_context = nysa_core::context::MessageContext::new(platform.clone());

    let user = nysa_core::context::UserContext::new(
        uuid::Uuid::new_v4(),
        message.author.id.to_string(),
        platform,
        message.author.name.clone(),
    );
    msg_context = msg_context.with_user(user);

    let now_ts = chrono::Utc::now().timestamp();
    let sent_ts = message.timestamp.unix_timestamp();
    let delta_secs = now_ts.saturating_sub(sent_ts);

    let relative_time = if delta_secs < 5 {
        "just now".to_string()
    } else if delta_secs < 60 {
        format!("{}s ago", delta_secs)
    } else if delta_secs < 60 * 60 {
        format!("{}m ago", delta_secs / 60)
    } else if delta_secs < 60 * 60 * 24 {
        format!("{}h ago", delta_secs / (60 * 60))
    } else {
        format!("{}d ago", delta_secs / (60 * 60 * 24))
    };

    let llm_input = format!(
        "({}, {}, {}): {}",
        message.author.name, platform_label, relative_time, message.content
    );

    let response = conversation_manager
        .send_message(thread_id, &llm_input, &msg_context, None)
        .await
        .map_err(|e| format!("LLM error: {}", e))?;

    Ok((response.content, message.id.get()))
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, DiscordData, Box<dyn std::error::Error + Send + Sync>>,
    data: &DiscordData,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event {
        serenity::FullEvent::Message { new_message } => {
            // Ignore bot messages
            if new_message.author.bot {
                return Ok(());
            }

            let bot_id = ctx.cache.current_user().id;
            let is_mention = new_message.mentions.iter().any(|m| m.id == bot_id);
            let is_dm = new_message.guild_id.is_none();
            let _channel_id = new_message.channel_id.get();
            let author_id = new_message.author.id.get();

            // Check authentication
            let auth_result = data
                .auth_middleware
                .authenticate(author_id, new_message.author.name.clone())
                .await;

            if auth_result.is_none() && !is_dm {
                // In guilds, require authentication unless mentioned (we'll show unauth message)
                if is_mention {
                    // Send unauth message
                    let unauth = data.message_handler.unauth_embed();
                    let embed = serenity::CreateEmbed::new()
                        .title(&unauth.title)
                        .description(&unauth.description)
                        .color(unauth.color);

                    new_message
                        .channel_id
                        .send_message(&ctx.http, serenity::CreateMessage::new().embed(embed))
                        .await?;
                }
                return Ok(());
            }

            let user_uuid = auth_result.as_ref().map(|u| u.user_id);

            // Handle based on channel mode and message type
            if is_dm {
                // DM handling
                handle_dm_message(ctx, new_message, data, user_uuid).await?;
            } else {
                // Guild channel handling
                handle_guild_message(ctx, new_message, data, user_uuid, is_mention).await?;
            }
        }
        serenity::FullEvent::InteractionCreate {
            interaction: serenity::Interaction::Component(component),
        } => {
            tracing::debug!(
                "Received component interaction: {:?}",
                component.data.custom_id
            );
        }
        _ => {}
    }

    Ok(())
}

async fn handle_dm_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &DiscordData,
    user_uuid: Option<uuid::Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user_id = message.author.id.get();
    let lock = data
        .thread_manager
        .get_or_create_processing_lock(message.channel_id.get())
        .await;
    let _guard = lock.lock().await;

    if let Some(uuid) = user_uuid {
        // Update DM handler state
        let _dm_thread_state = data.dm_handler.get_or_create_thread(user_id, uuid).await;

        // Ensure ThreadManager has a corresponding DM conversational thread and reuse it
        let thread_state = data
            .thread_manager
            .get_or_create_dm_thread(user_id, uuid)
            .await;

        // Check if we should respond based on DM mode
        let should_respond = match data.config.dm_mode {
            DmMode::Reactive => true,
            DmMode::Proactive => {
                let is_proactive = data.proactive_manager.should_send_message(user_id).await;
                data.dm_handler.should_respond(user_id, is_proactive).await
            }
        };

        if should_respond {
            data.dm_handler.update_activity(user_id).await;
            data.proactive_manager.record_message(user_id).await;

            match process_message_with_llm(ctx, message, data, thread_state.id).await {
                Ok((response, _user_msg_id)) => {
                    let bot_message = message
                        .channel_id
                        .send_message(&ctx.http, serenity::CreateMessage::new().content(&response))
                        .await?;

                    // Track bot's message in thread by UUID
                    let _ = data
                        .thread_manager
                        .add_message_to_thread_by_uuid(thread_state.id, bot_message.id.get())
                        .await;

                    tracing::info!(
                        "DM response sent to {} (thread: {})",
                        message.author.name,
                        thread_state.id
                    );
                }
                Err(e) => {
                    tracing::error!("Failed to process DM message: {}", e);
                    message
                        .channel_id
                        .send_message(
                            &ctx.http,
                            serenity::CreateMessage::new()
                                .content("Sorry, I encountered an error processing your message."),
                        )
                        .await?;
                }
            }
        }
    } else {
        // Not authenticated - send unauth message
        let unauth = data.message_handler.unauth_embed();
        let embed = serenity::CreateEmbed::new()
            .title(&unauth.title)
            .description(&unauth.description)
            .color(unauth.color);

        message
            .channel_id
            .send_message(&ctx.http, serenity::CreateMessage::new().embed(embed))
            .await?;
    }

    Ok(())
}

async fn handle_guild_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &DiscordData,
    user_uuid: Option<uuid::Uuid>,
    is_mention: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let channel_id = message.channel_id.get();
    let guild_id = message.guild_id.map(|g| g.get());

    let lock = data
        .thread_manager
        .get_or_create_processing_lock(channel_id)
        .await;
    let _guard = lock.lock().await;

    // Get channel mode
    let mode = data
        .message_handler
        .get_channel_mode(channel_id, guild_id)
        .await;

    match mode {
        ChannelMode::Disabled => {
            // Don't respond in disabled mode
            return Ok(());
        }
        ChannelMode::Thread => {
            if is_mention {
                // Check if there's already an active thread in this channel
                let existing_thread = data.thread_manager.get_thread(channel_id).await;

                if let Some(thread) = existing_thread {
                    // Continue in existing thread instead of creating new one
                    tracing::info!(
                        "Reusing existing thread {} in channel {} for mention from {}",
                        thread.id,
                        channel_id,
                        message.author.name
                    );

                    data.thread_manager.update_activity(channel_id).await;
                    data.thread_manager
                        .add_message_to_thread(channel_id, message.id.get())
                        .await;

                    if let Some(_uuid) = user_uuid {
                        match process_message_with_llm(ctx, message, data, thread.id).await {
                            Ok((response, _user_msg_id)) => {
                                let bot_message = message
                                    .channel_id
                                    .send_message(
                                        &ctx.http,
                                        serenity::CreateMessage::new().content(&response),
                                    )
                                    .await?;

                                data.thread_manager
                                    .add_message_to_thread(channel_id, bot_message.id.get())
                                    .await;

                                tracing::info!(
                                    "Thread {} response sent to {}",
                                    thread.id,
                                    message.author.name
                                );
                            }
                            Err(e) => {
                                tracing::error!("Failed to process mention message: {}", e);

                                let embed = serenity::CreateEmbed::new()
                                    .title("Error")
                                    .description("I encountered an error processing your message. Please try again.")
                                    .color(0xFF6B6B);

                                message
                                    .channel_id
                                    .send_message(
                                        &ctx.http,
                                        serenity::CreateMessage::new().embed(embed),
                                    )
                                    .await?;
                            }
                        }
                    }
                } else if let Some(uuid) = user_uuid {
                    // No existing thread, create new one
                    let thread = data
                        .thread_manager
                        .create_from_mention(channel_id, message.id.get(), uuid)
                        .await;

                    match process_message_with_llm(ctx, message, data, thread.id).await {
                        Ok((response, _user_msg_id)) => {
                            let bot_message = message
                                .channel_id
                                .send_message(
                                    &ctx.http,
                                    serenity::CreateMessage::new().content(&response),
                                )
                                .await?;

                            data.thread_manager
                                .add_message_to_thread(channel_id, bot_message.id.get())
                                .await;

                            tracing::info!(
                                "Thread {} created and responded to {}",
                                thread.id,
                                message.author.name
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to process mention message: {}", e);

                            let embed = serenity::CreateEmbed::new()
                                .title("Error")
                                .description("I encountered an error processing your message. Please try again.")
                                .color(0xFF6B6B);

                            message
                                .channel_id
                                .send_message(
                                    &ctx.http,
                                    serenity::CreateMessage::new().embed(embed),
                                )
                                .await?;
                        }
                    }
                }
            } else {
                // Check if this is a reply to a message in an active thread
                let parent_message_id = message.referenced_message.as_ref().map(|m| m.id.get());

                tracing::debug!(
                    "Non-mention message in channel {}, has_reference={}, parent_id={:?}",
                    channel_id,
                    message.referenced_message.is_some(),
                    parent_message_id
                );

                if let Some(parent_id) = parent_message_id {
                    let thread = data.thread_manager.check_reply_chain(parent_id).await;
                    if let Some(thread_state) = thread {
                        // Continue in existing thread
                        let _ = data
                            .thread_manager
                            .update_activity_by_uuid(thread_state.id)
                            .await;
                        // Add user's message to thread
                        let _ = data
                            .thread_manager
                            .add_message_to_thread_by_uuid(thread_state.id, message.id.get())
                            .await;

                        if let Some(_uuid) = user_uuid {
                            match process_message_with_llm(ctx, message, data, thread_state.id)
                                .await
                            {
                                Ok((response, _user_msg_id)) => {
                                    let bot_message = message
                                        .channel_id
                                        .send_message(
                                            &ctx.http,
                                            serenity::CreateMessage::new().content(&response),
                                        )
                                        .await?;

                                    // Track bot's message in thread
                                    let _ = data
                                        .thread_manager
                                        .add_message_to_thread_by_uuid(
                                            thread_state.id,
                                            bot_message.id.get(),
                                        )
                                        .await;

                                    tracing::info!(
                                        "Thread {} response sent to {}",
                                        thread_state.id,
                                        message.author.name
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("Failed to process thread message: {}", e);
                                }
                            }
                        }
                    } else {
                        tracing::debug!(
                            "No active thread found for parent message {}, ignoring",
                            parent_id
                        );
                    }
                }
            }
        }
        ChannelMode::Active => {
            // Thread mode + proactive
            if is_mention {
                // Handle mention like Thread mode
                if let Some(uuid) = user_uuid {
                    let thread = data
                        .thread_manager
                        .create_from_mention(channel_id, message.id.get(), uuid)
                        .await;

                    match process_message_with_llm(ctx, message, data, thread.id).await {
                        Ok((response, _user_msg_id)) => {
                            let bot_message = message
                                .channel_id
                                .send_message(
                                    &ctx.http,
                                    serenity::CreateMessage::new().content(&response),
                                )
                                .await?;

                            let _ = data
                                .thread_manager
                                .add_message_to_thread_by_uuid(thread.id, bot_message.id.get())
                                .await;

                            tracing::info!(
                                "Thread {} created and responded to {}",
                                thread.id,
                                message.author.name
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to process mention message: {}", e);
                        }
                    }
                }
            } else {
                // Check if this is a reply to a message in an active thread
                let parent_message_id = message.referenced_message.as_ref().map(|m| m.id.get());

                if let Some(parent_id) = parent_message_id {
                    let thread = data.thread_manager.check_reply_chain(parent_id).await;
                    if let Some(thread_state) = thread {
                        let _ = data
                            .thread_manager
                            .update_activity_by_uuid(thread_state.id)
                            .await;

                        if let Some(_uuid) = user_uuid {
                            match process_message_with_llm(ctx, message, data, thread_state.id)
                                .await
                            {
                                Ok((response, _user_msg_id)) => {
                                    let bot_message = message
                                        .channel_id
                                        .send_message(
                                            &ctx.http,
                                            serenity::CreateMessage::new().content(&response),
                                        )
                                        .await?;

                                    data.thread_manager
                                        .add_message_to_thread(channel_id, bot_message.id.get())
                                        .await;

                                    tracing::info!(
                                        "Thread {} response sent to {}",
                                        thread_state.id,
                                        message.author.name
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("Failed to process thread message: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            // Check for proactive response opportunity
            if let Some(uuid) = user_uuid {
                let author_id = message.author.id.get();
                let should_proactive = data.proactive_manager.should_send_message(author_id).await;

                if should_proactive {
                    data.proactive_manager.register_user(uuid, author_id).await;

                    let thread_state = match data.thread_manager.get_thread(channel_id).await {
                        Some(existing) => existing,
                        None => {
                            data.thread_manager
                                .create_from_mention(channel_id, message.id.get(), uuid)
                                .await
                        }
                    };

                    match process_message_with_llm(ctx, message, data, thread_state.id).await {
                        Ok((response, _user_msg_id)) => {
                            let bot_message = message
                                .channel_id
                                .send_message(
                                    &ctx.http,
                                    serenity::CreateMessage::new().content(&response),
                                )
                                .await?;

                            data.thread_manager
                                .add_message_to_thread(channel_id, bot_message.id.get())
                                .await;

                            tracing::info!(
                                "Proactive response sent to {} in channel {}",
                                message.author.name,
                                channel_id
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to generate proactive response: {}", e);
                        }
                    }
                }

                data.proactive_manager.record_message(author_id).await;
            }
        }
        ChannelMode::EvaluateAll => {
            // Evaluate every message
            let should_respond = data
                .evaluate_handler
                .should_respond(
                    channel_id,
                    &message.content,
                    message.author.id.get(),
                    0.6, // threshold
                )
                .await;

            if should_respond {
                data.evaluate_handler
                    .record_message(channel_id, message.content.clone(), message.author.id.get())
                    .await;
                data.evaluate_handler.mark_responded(channel_id).await;

                if let Some(uuid) = user_uuid {
                    let thread_state = match data.thread_manager.get_thread(channel_id).await {
                        Some(existing) => existing,
                        None => {
                            data.thread_manager
                                .create_from_mention(channel_id, message.id.get(), uuid)
                                .await
                        }
                    };

                    match process_message_with_llm(ctx, message, data, thread_state.id).await {
                        Ok((response, _user_msg_id)) => {
                            let bot_message = message
                                .channel_id
                                .send_message(
                                    &ctx.http,
                                    serenity::CreateMessage::new().content(&response),
                                )
                                .await?;

                            data.thread_manager
                                .add_message_to_thread(channel_id, bot_message.id.get())
                                .await;

                            tracing::info!(
                                "EvaluateAll response sent to {} in channel {}",
                                message.author.name,
                                channel_id
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to generate EvaluateAll response: {}", e);
                        }
                    }
                }
            }
        }
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
            "You are interacting with a user on Discord. Discord has the following features available:\n\
            - Slash commands (/) for user interactions\n\
            - Threaded conversations for organized discussions\n\
            - Message reactions (emoji) for quick responses\n\
            - Voice channels (in development)\n\
            - Direct messages (DMs) for private conversations\n\
            - Server-specific permissions and roles",
        ));

        if ctx.platform == Platform::DiscordDm {
            sections.push(PromptSection::new(
                "dm_context",
                20,
                "You are in a direct message (DM) conversation with the user. Treat this as a more personal conversation. \
                The user has chosen to message you directly, so they likely want focused assistance or conversation.",
            ));
        } else {
            sections.push(PromptSection::new(
                "guild_context",
                15,
                "You are in a Discord server (guild). Be mindful that conversations may be public and visible to others. \
                Use threads when appropriate to keep discussions organized.",
            ));
        }

        sections
    }
}
