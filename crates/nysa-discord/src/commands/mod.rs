use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::models::user::Entity as UserEntity;
use nysa_core::generate_token;
use sea_orm::{ActiveValue, EntityTrait, Set};

type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, crate::DiscordData, Error>;

/// Authenticate with Nysa or link accounts
#[poise::command(slash_command, prefix_command)]
pub async fn auth(
    ctx: Context<'_>,
    #[description = "Your nysa token or linking code"] token_or_code: Option<String>,
) -> Result<(), Error> {
    let db = ctx.data().db.clone();
    let discord_id = ctx.author().id.get();
    let username = ctx.author().name.clone();

    // Check if user is already authenticated by searching for discord ID in linked_profiles
    let users = UserEntity::find().all(&db).await?;

    let existing_user = users.into_iter().find(|u| {
        if let Some(profiles) = u.linked_profiles.as_object() {
            if let Some(discord) = profiles.get("discord") {
                if let Some(id) = discord.get("id") {
                    if let Some(id_str) = id.as_str() {
                        return id_str == discord_id.to_string();
                    }
                }
            }
        }
        false
    });

    if existing_user.is_some() {
        // User is already authenticated
        let embed = serenity::CreateEmbed::new()
            .title("Already Authenticated")
            .description("You're already authenticated with Nysa!")
            .color(0x4ADE80);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    if let Some(input) = token_or_code {
        let input = input.trim();

        if input.starts_with("nysa_") {
            // User provided a token - validate and link
            let embed = serenity::CreateEmbed::new()
                .title("Token Received")
                .description("Token linking not yet fully implemented. Creating new account...")
                .color(0xFFB84D);

            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await?;
        } else {
            // Assume it's a linking code
            let embed = serenity::CreateEmbed::new()
                .title("Linking Code")
                .description("Linking codes are not yet implemented.")
                .color(0xFFB84D);

            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await?;
        }
    } else {
        // No token provided - create new user
        let token = generate_token();
        let user_id = Uuid::new_v4();

        // Create user in database
        let user = crate::models::user::ActiveModel {
            id: Set(user_id),
            created_at: Set(chrono::Utc::now().naive_utc()),
            linked_profiles: Set(serde_json::json!({
                "discord": {
                    "id": discord_id.to_string(),
                    "username": username,
                }
            })),
            preferences: Set(serde_json::json!({})),
            token_hash: Set(token.clone()), // In production, this should be hashed
        };

        UserEntity::insert(user).exec(&db).await?;

        let embed = serenity::CreateEmbed::new()
            .title("Welcome to Nysa!")
            .description(format!(
                "Your Discord account has been linked.\n\n**Your Token:**\n||`{}`||\n\n**Important:** Keep this token safe! You'll need it to authenticate on other platforms.",
                token
            ))
            .color(0x4ADE80)
            .footer(serenity::CreateEmbedFooter::new("Never share this token publicly!"));

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
    }

    Ok(())
}

/// Compact thread context to save tokens
#[poise::command(slash_command, prefix_command)]
pub async fn compact(
    ctx: Context<'_>,
    #[description = "Thread ID to compact (optional)"] _thread_id: Option<String>,
) -> Result<(), Error> {
    ctx.say("Compaction service is not yet fully implemented. This feature will compress thread context to save tokens.").await?;
    Ok(())
}

/// Start a new conversation thread
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn newthread(
    ctx: Context<'_>,
    #[description = "Optional thread name"] name: Option<String>,
) -> Result<(), Error> {
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

    let embed = serenity::CreateEmbed::new()
        .title("New Thread Started")
        .description(format!("Created thread: {}", thread_name))
        .color(0x4ADE80);

    thread
        .send_message(
            &ctx.serenity_context().http,
            serenity::CreateMessage::new().embed(embed),
        )
        .await?;

    Ok(())
}
