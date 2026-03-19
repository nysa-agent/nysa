use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;

use crate::models::user::Entity as UserEntity;
use nysa_core::auth::LinkingCodeError;
use nysa_core::{AuthError, AuthService};
use sea_orm::EntityTrait;

type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, crate::DiscordData, Error>;

/// Check if a Discord user is already authenticated
async fn check_existing_user(
    db: &DatabaseConnection,
    discord_id: u64,
) -> Result<Option<crate::models::user::Model>, Error> {
    let users = UserEntity::find().all(db).await?;

    Ok(users.into_iter().find(|u| {
        if let Some(profiles) = u.linked_profiles.as_object()
            && let Some(discord) = profiles.get("discord")
            && let Some(id) = discord.get("id")
            && let Some(id_str) = id.as_str()
        {
            return id_str == discord_id.to_string();
        }
        false
    }))
}

/// Authenticate with Nysa or link accounts
#[poise::command(slash_command, prefix_command, dm_only = false)]
pub async fn auth(
    ctx: Context<'_>,
    #[description = "Your nysa token or linking code"] token_or_code: Option<String>,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();
    let username = ctx.author().name.clone();
    let auth_service = AuthService::new(db.clone());

    // Check rate limit
    let rate_limit_key = format!("discord:{}", discord_id);
    let rate_limit_result = auth_service.check_rate_limit(&rate_limit_key);

    if !rate_limit_result.allowed {
        let retry_after = rate_limit_result
            .retry_after
            .map(|d| format!("{} seconds", d.as_secs()))
            .unwrap_or_else(|| "a while".to_string());

        let embed = serenity::CreateEmbed::new()
            .title("Rate Limited")
            .description(format!(
                "Too many authentication attempts. Please try again in {}.",
                retry_after
            ))
            .color(0xFF6B6B);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    // Record the attempt
    auth_service.record_auth_attempt(&rate_limit_key);

    // Check if user is already authenticated
    let existing_user = check_existing_user(&db, discord_id).await?;

    if let Some(user) = existing_user {
        // User is already authenticated - show account info
        let profiles = auth_service.get_user_profiles(&user.id).await?;

        let linked_platforms = profiles
            .as_object()
            .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| "None".to_string());

        let embed = serenity::CreateEmbed::new()
            .title("Already Authenticated")
            .description(format!(
                "You're already authenticated with Nysa!\n\n**Linked Platforms:** {}\n\nUse `/settings` to manage your account.",
                linked_platforms
            ))
            .color(0x4ADE80);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    if let Some(input) = token_or_code {
        let input = input.trim();

        if input.starts_with("nysa_") {
            // Flow 2: User provided a token - validate and link
            match auth_service.authenticate(input).await {
                Ok(user_id) => {
                    // Valid token - link Discord to existing account
                    let metadata = serde_json::json!({
                        "username": username,
                        "discriminator": ctx.author().discriminator.map(|d| d.get().to_string()),
                        "avatar": ctx.author().avatar_url(),
                    });

                    match auth_service
                        .link_platform(user_id, "discord", &discord_id.to_string(), metadata)
                        .await
                    {
                        Ok(()) => {
                            // Create session
                            let session_metadata = serde_json::json!({
                                "guild_id": ctx.guild_id().map(|g| g.get()),
                                "channel_id": ctx.channel_id().get(),
                            });

                            let _session = auth_service
                                .create_session(
                                    user_id,
                                    "discord",
                                    &format!("{}", discord_id),
                                    session_metadata,
                                )
                                .await;

                            let embed = serenity::CreateEmbed::new()
                                .title("Account Linked!")
                                .description("Your Discord account has been linked to your existing Nysa account.\n\n**Linked Platforms:** discord, and any others you had before.\n\nWelcome back!")
                                .color(0x4ADE80);

                            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                                .await?;
                        }
                        Err(AuthError::PlatformAlreadyLinked) => {
                            let embed = serenity::CreateEmbed::new()
                                .title("Already Linked")
                                .description("This Discord account is already linked to a different Nysa account.")
                                .color(0xFFB84D);

                            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                                .await?;
                        }
                        Err(e) => {
                            tracing::error!("Failed to link platform: {}", e);
                            let embed = serenity::CreateEmbed::new()
                                .title("Link Failed")
                                .description("An error occurred while linking your account. Please try again.")
                                .color(0xFF6B6B);

                            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                                .await?;
                        }
                    }
                }
                Err(AuthError::InvalidToken) => {
                    let embed = serenity::CreateEmbed::new()
                        .title("Invalid Token")
                        .description("The token you provided is invalid or has been revoked. Please check your token and try again.")
                        .color(0xFF6B6B);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
                Err(e) => {
                    tracing::error!("Authentication error: {}", e);
                    let embed = serenity::CreateEmbed::new()
                        .title("Authentication Error")
                        .description(
                            "An error occurred during authentication. Please try again later.",
                        )
                        .color(0xFF6B6B);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
            }
        } else {
            // Flow 3: Assume it's a linking code
            let metadata = serde_json::json!({
                "username": username,
                "discriminator": ctx.author().discriminator.map(|d| d.get().to_string()),
                "avatar": ctx.author().avatar_url(),
            });

            match auth_service
                .redeem_linking_code(input, "discord", &discord_id.to_string(), metadata)
                .await
            {
                Ok(user_id) => {
                    // Create session
                    let session_metadata = serde_json::json!({
                        "guild_id": ctx.guild_id().map(|g| g.get()),
                        "channel_id": ctx.channel_id().get(),
                    });

                    let _session = auth_service
                        .create_session(
                            user_id,
                            "discord",
                            &format!("{}", discord_id),
                            session_metadata,
                        )
                        .await;

                    let embed = serenity::CreateEmbed::new()
                        .title("Account Linked!")
                        .description("Your Discord account has been linked using the linking code.\n\nYou can now use Nysa across all your linked platforms!")
                        .color(0x4ADE80);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
                Err(AuthError::InvalidLinkingCode(LinkingCodeError::InvalidCode)) => {
                    let embed = serenity::CreateEmbed::new()
                        .title("Invalid Linking Code")
                        .description("The linking code you provided is invalid. Please check the code and try again.")
                        .color(0xFF6B6B);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
                Err(AuthError::InvalidLinkingCode(LinkingCodeError::Expired)) => {
                    let embed = serenity::CreateEmbed::new()
                        .title("Expired Linking Code")
                        .description("This linking code has expired. Linking codes are valid for 5 minutes. Please generate a new one.")
                        .color(0xFFB84D);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
                Err(AuthError::InvalidLinkingCode(LinkingCodeError::AlreadyUsed)) => {
                    let embed = serenity::CreateEmbed::new()
                        .title("Code Already Used")
                        .description("This linking code has already been used. Each code can only be used once.")
                        .color(0xFFB84D);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
                Err(e) => {
                    tracing::error!("Linking code error: {}", e);
                    let embed = serenity::CreateEmbed::new()
                        .title("Link Failed")
                        .description(
                            "An error occurred while redeeming the linking code. Please try again.",
                        )
                        .color(0xFF6B6B);

                    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                        .await?;
                }
            }
        }
    } else {
        // Flow 1: No token provided - create new user
        match auth_service.create_user().await {
            Ok((user_id, token)) => {
                // Link Discord to the new account
                let metadata = serde_json::json!({
                    "username": username,
                    "discriminator": ctx.author().discriminator.map(|d| d.get().to_string()),
                    "avatar": ctx.author().avatar_url(),
                });

                if let Err(e) = auth_service
                    .link_platform(user_id, "discord", &discord_id.to_string(), metadata)
                    .await
                {
                    tracing::error!("Failed to link Discord to new user: {}", e);
                }

                // Create session
                let session_metadata = serde_json::json!({
                    "guild_id": ctx.guild_id().map(|g| g.get()),
                    "channel_id": ctx.channel_id().get(),
                });

                let _session = auth_service
                    .create_session(
                        user_id,
                        "discord",
                        &format!("{}", discord_id),
                        session_metadata,
                    )
                    .await;

                let embed = serenity::CreateEmbed::new()
                    .title("Welcome to Nysa!")
                    .description(format!(
                        "Your new Nysa account has been created and Discord has been linked.\n\n**Your Token:**\n||`{}`||\n\n**Important:** Keep this token safe! You'll need it to authenticate on other platforms.\n\nYou can use this token with the `/auth` command or on other platforms to link them to this account.",
                        token
                    ))
                    .color(0x4ADE80)
                    .footer(serenity::CreateEmbedFooter::new("Never share this token publicly!"));

                ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
            }
            Err(e) => {
                tracing::error!("Failed to create user: {}", e);
                let embed = serenity::CreateEmbed::new()
                    .title("Account Creation Failed")
                    .description(
                        "An error occurred while creating your account. Please try again later.",
                    )
                    .color(0xFF6B6B);

                ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
            }
        }
    }

    Ok(())
}

/// Generate a linking code for cross-platform authentication
#[poise::command(slash_command, prefix_command, rename = "link")]
pub async fn generate_link(ctx: Context<'_>) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();
    let auth_service = AuthService::new(db.clone());

    // Find the user
    let existing_user = check_existing_user(&db, discord_id).await?;

    if let Some(user) = existing_user {
        match auth_service.generate_linking_code(user.id, "discord").await {
            Ok(code) => {
                let embed = serenity::CreateEmbed::new()
                    .title("Linking Code Generated")
                    .description(format!(
                        "Your linking code is:\n\n**`{}`**\n\nThis code expires in **5 minutes** and can only be used once.\n\nUse this code on another platform to link it to your Nysa account.",
                        code
                    ))
                    .color(0x4ADE80)
                    .footer(serenity::CreateEmbedFooter::new("Keep this code private!"));

                ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
            }
            Err(AuthError::PlatformAlreadyLinked) => {
                let embed = serenity::CreateEmbed::new()
                    .title("Already Linked")
                    .description("This platform is already linked to your account. You cannot generate a linking code for it.")
                    .color(0xFFB84D);

                ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
            }
            Err(e) => {
                tracing::error!("Failed to generate linking code: {}", e);
                let embed = serenity::CreateEmbed::new()
                    .title("Error")
                    .description("Failed to generate linking code. Please try again.")
                    .color(0xFF6B6B);

                ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
            }
        }
    } else {
        let embed = serenity::CreateEmbed::new()
            .title("Not Authenticated")
            .description("You need to authenticate first using `/auth` before you can generate a linking code.")
            .color(0xFF6B6B);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
    }

    Ok(())
}

/// Compact thread context to save tokens
#[poise::command(slash_command, prefix_command)]
pub async fn compact(
    ctx: Context<'_>,
    #[description = "Thread ID to compact (optional)"] thread_id: Option<String>,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();

    // Check if user is authenticated
    let existing_user = check_existing_user(&db, discord_id).await?;

    if existing_user.is_none() {
        let embed = serenity::CreateEmbed::new()
            .title("Authentication Required")
            .description("You need to authenticate first using `/auth` to use this command.")
            .color(0xFF6B6B);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    // TODO: Implement actual compaction logic
    if let Some(thread_id) = thread_id {
        ctx.say(format!(
            "Compacting thread: {} (Not yet implemented)",
            thread_id
        ))
        .await?;
    } else {
        ctx.say("Compacting current thread... (Not yet implemented)")
            .await?;
    }

    Ok(())
}

/// Start a new conversation thread
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn newthread(
    ctx: Context<'_>,
    #[description = "Optional thread name"] name: Option<String>,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();

    // Check if user is authenticated
    let existing_user = check_existing_user(&db, discord_id).await?;

    if existing_user.is_none() {
        let embed = serenity::CreateEmbed::new()
            .title("Authentication Required")
            .description("You need to authenticate first using `/auth` to use this command.")
            .color(0xFF6B6B);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    let channel = ctx.channel_id();
    let thread_name = name.unwrap_or_else(|| "Chat with Nysa".to_string());

    let thread = channel
        .create_thread(
            &ctx.serenity_context().http,
            serenity::CreateThread::new(&thread_name)
                .kind(serenity::ChannelType::PublicThread)
                .invitable(false),
        )
        .await?;

    let user = existing_user.unwrap();
    let thread_state = ctx
        .data()
        .thread_manager
        .register_channel_thread(thread.id.get(), user.id, None)
        .await;

    let embed = serenity::CreateEmbed::new()
        .title("New Thread Started")
        .description(format!(
            "Created thread: {}\nConversation ID: `{}`",
            thread_name, thread_state.id
        ))
        .color(0x4ADE80);

    thread
        .send_message(
            &ctx.serenity_context().http,
            serenity::CreateMessage::new().embed(embed),
        )
        .await?;

    Ok(())
}

/// Display help information
#[poise::command(slash_command, prefix_command)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    let embed = serenity::CreateEmbed::new()
        .title("Nysa Help")
        .description("Here are the available commands:")
        .field(
            "/auth [token_or_code]",
            "Authenticate with Nysa or link your Discord account to an existing account.\n• No argument: Create new account\n• With nysa_* token: Link to existing account\n• With linking code: Link using code from another platform",
            false,
        )
        .field(
            "/link",
            "Generate a linking code to connect other platforms to your Nysa account.",
            false,
        )
        .field(
            "/compact",
            "Compact conversation thread to save tokens. (Coming soon)",
            false,
        )
        .field(
            "/newthread",
            "Start a new conversation thread in this channel.",
            false,
        )
        .field(
            "/settings",
            "Manage your Nysa settings and preferences. (Coming soon)",
            false,
        )
        .field(
            "/help",
            "Show this help message.",
            false,
        )
        .color(0x4ADE80);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Manage user settings
#[poise::command(slash_command, prefix_command, rename = "settings")]
pub async fn settings(ctx: Context<'_>) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();

    // Check if user is authenticated
    let existing_user = check_existing_user(&db, discord_id).await?;

    if existing_user.is_none() {
        let embed = serenity::CreateEmbed::new()
            .title("Authentication Required")
            .description("You need to authenticate first using `/auth` to view your settings.")
            .color(0xFF6B6B);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    // TODO: Implement settings view/edit
    let embed = serenity::CreateEmbed::new()
        .title("Settings")
        .description("Settings management is coming soon. You'll be able to:\n• View linked platforms\n• Manage notification preferences\n• Update your profile\n• View token information")
        .color(0xFFB84D);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
