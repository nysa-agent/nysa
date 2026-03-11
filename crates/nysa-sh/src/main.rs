use std::path::PathBuf;

use clap::Parser;
use nysa_core::App;
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

    // Clone the database connection for DiscordExtension
    let db_clone = db.clone();

    let app = App::builder(db)
        .extension(DiscordExtension::new(discord_config, db_clone))
        .build()
        .await?;

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
