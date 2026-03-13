use std::path::PathBuf;

use clap::Parser;
use nysa_core::App;
use nysa_core::config::{AiConfigBuilder, ChatConfigBuilder, EmbeddingConfigBuilder};
use nysa_discord::models::{ChannelMode, DmMode};
use nysa_discord::{DiscordExtension, DiscordExtensionConfig, UnauthMessage};
use sea_orm::Database;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name = "nysa")]
#[command(about = "Nysa AI Agent Framework", long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    database: DatabaseConfig,
    discord: DiscordConfigSection,
    ai: Option<AiConfigSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct DatabaseConfig {
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordConfigSection {
    token: String,
    application_id: u64,
    default_mode: Option<String>,
    proactive_min: Option<i64>,
    proactive_max: Option<i64>,
    dm_mode: Option<String>,
    unauth_message: Option<UnauthMessageConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct UnauthMessageConfig {
    title: String,
    description: String,
    color: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct AiConfigSection {
    chat: ChatConfigSection,
    embedding: Option<EmbeddingConfigSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatConfigSection {
    base_url: String,
    api_key: String,
    model: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_completion_tokens: Option<u32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingConfigSection {
    base_url: String,
    api_key: String,
    model: String,
    dimensions: Option<u32>,
}

impl From<DiscordConfigSection> for DiscordExtensionConfig {
    fn from(config: DiscordConfigSection) -> Self {
        let default_mode = match config.default_mode.as_deref() {
            Some("disabled") => ChannelMode::Disabled,
            Some("evaluate_all") => ChannelMode::EvaluateAll,
            Some("thread") => ChannelMode::Thread,
            Some("active") => ChannelMode::Active,
            _ => ChannelMode::Thread,
        };

        let dm_mode = match config.dm_mode.as_deref() {
            Some("reactive") => DmMode::Reactive,
            Some("proactive") => DmMode::Proactive,
            _ => DmMode::Reactive,
        };

        let unauth_message = config
            .unauth_message
            .map(|m| UnauthMessage {
                title: m.title,
                description: m.description,
                color: m.color.unwrap_or(0xFF6B6B),
            })
            .unwrap_or(UnauthMessage {
                title: "Authentication Required".to_string(),
                description: "Please authenticate with Nysa using `/auth` to start chatting."
                    .to_string(),
                color: 0xFF6B6B,
            });

        DiscordExtensionConfig {
            token: config.token,
            application_id: config.application_id,
            default_mode,
            proactive_min: config.proactive_min.unwrap_or(60),
            proactive_max: config.proactive_max.unwrap_or(240),
            dm_mode,
            unauth_message,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config_content = std::fs::read_to_string(&args.config).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read config file {}: {}",
            args.config.display(),
            e
        )
    })?;

    let config: Config = toml::from_str(&config_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

    // Note: Logging is initialized by nysa-core App::init_logging()

    tracing::info!("Connecting to database...");
    let db = Database::connect(&config.database.url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    let discord_config: DiscordExtensionConfig = config.discord.into();

    tracing::info!("Starting Nysa...");

    let db_clone = db.clone();

    let mut app_builder = App::builder(db).extension(DiscordExtension::new(discord_config, db_clone));

    // Configure AI if provided
    if let Some(ai_config) = config.ai {
        tracing::info!("Configuring AI with model: {}", ai_config.chat.model);
        
        let mut chat_builder = ChatConfigBuilder::new()
            .base_url(ai_config.chat.base_url)
            .api_key(ai_config.chat.api_key)
            .model(ai_config.chat.model);

        if let Some(temp) = ai_config.chat.temperature {
            chat_builder = chat_builder.temperature(temp);
        }
        if let Some(top_p) = ai_config.chat.top_p {
            chat_builder = chat_builder.top_p(top_p);
        }
        if let Some(max_tokens) = ai_config.chat.max_completion_tokens {
            chat_builder = chat_builder.max_completion_tokens(max_tokens);
        }
        if let Some(freq_pen) = ai_config.chat.frequency_penalty {
            chat_builder = chat_builder.frequency_penalty(freq_pen);
        }
        if let Some(pres_pen) = ai_config.chat.presence_penalty {
            chat_builder = chat_builder.presence_penalty(pres_pen);
        }

        let chat_config = chat_builder.build()?;

        let mut ai_builder = AiConfigBuilder::new().chat(chat_config);

        if let Some(embedding) = ai_config.embedding {
            let mut embedding_builder = EmbeddingConfigBuilder::new()
                .base_url(embedding.base_url)
                .api_key(embedding.api_key)
                .model(embedding.model);

            if let Some(dims) = embedding.dimensions {
                embedding_builder = embedding_builder.dimensions(dims);
            }

            ai_builder = ai_builder.embedding(embedding_builder.build()?);
        }

        app_builder = app_builder.ai(ai_builder.build()?);
    } else {
        tracing::warn!("No AI configuration found. LLM features will be disabled.");
    }

    let app = app_builder.build().await?;

    tracing::info!("Registered tools:");
    {
        let registry = app.tool_registry();
        let registry = registry.read().await;
        for tool in registry.all() {
            tracing::info!("  {} ({}) - {}", tool.name, tool.category, tool.description);
        }
    }

    tracing::info!("Nysa is running. Press Ctrl+C to shut down.");

    tokio::signal::ctrl_c().await?;

    tracing::info!("Shutting down...");
    app.shutdown().await?;

    Ok(())
}
