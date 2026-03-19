use std::path::PathBuf;

use clap::Parser;
use nysa_core::config::{AiConfigBuilder, Provider};
use nysa_core::{App, ToolsReady};
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
    base_url: String,
    api_key: String,
    chat_model: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_completion_tokens: Option<u32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    embedding: Option<EmbeddingConfigSection>,
    compaction: Option<CompactionConfigSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingConfigSection {
    base_url: Option<String>,
    api_key: Option<String>,
    model: String,
    dimensions: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct CompactionConfigSection {
    enabled: Option<bool>,
    auto_threshold: Option<f32>,
    max_messages_to_summarize: Option<usize>,
    preserve_recent: Option<usize>,
    summary_model: Option<String>,
    provider: Option<ProviderSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderSection {
    base_url: Option<String>,
    api_key: Option<String>,
    #[allow(dead_code)]
    summary_model: Option<String>,
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

    let mut app_builder =
        App::builder(db).extension(DiscordExtension::new(discord_config, db_clone));

    // Configure AI if provided
    if let Some(ai_config) = config.ai {
        tracing::info!("Configuring AI with model: {}", ai_config.chat_model);

        let provider = Provider::new("main", ai_config.base_url, ai_config.api_key);

        let chat = nysa_core::config::ai::ChatConfig {
            provider: None,
            model: ai_config.chat_model.clone(),
            options: nysa_core::config::ai::ChatOptions {
                temperature: ai_config.temperature,
                top_p: ai_config.top_p,
                max_completion_tokens: ai_config.max_completion_tokens,
                frequency_penalty: ai_config.frequency_penalty,
                presence_penalty: ai_config.presence_penalty,
                stop_sequences: vec![],
            },
        };

        let embedding_config: Option<nysa_core::config::ai::EmbeddingConfig> =
            if let Some(embedding) = ai_config.embedding {
                let embedding_provider = if let (Some(base_url), Some(api_key)) =
                    (&embedding.base_url, &embedding.api_key)
                {
                    Some(Provider::new(
                        "embedding",
                        base_url.clone(),
                        api_key.clone(),
                    ))
                } else {
                    None
                };

                Some(nysa_core::config::ai::EmbeddingConfig {
                    provider: embedding_provider,
                    model: embedding.model.clone(),
                    dimensions: embedding.dimensions,
                    encoding_format: None,
                })
            } else {
                None
            };

        let compaction_config: nysa_core::config::ai::CompactionConfig =
            if let Some(compaction) = ai_config.compaction {
                let compaction_provider = if let Some(ref prov) = compaction.provider {
                    if let (Some(base_url), Some(api_key)) = (&prov.base_url, &prov.api_key) {
                        Some(Provider::new(
                            "compaction",
                            base_url.clone(),
                            api_key.clone(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };

                nysa_core::config::ai::CompactionConfig {
                    enabled: compaction.enabled.unwrap_or(true),
                    auto_threshold: compaction.auto_threshold.unwrap_or(0.75),
                    max_messages_to_summarize: compaction.max_messages_to_summarize.unwrap_or(50),
                    preserve_recent: compaction.preserve_recent.unwrap_or(10),
                    summary_model: compaction.summary_model,
                    provider: compaction_provider,
                }
            } else {
                nysa_core::config::ai::CompactionConfig::default()
            };

        let ai_builder = AiConfigBuilder::new()
            .provider(provider)
            .chat(chat)
            .embedding(
                embedding_config.unwrap_or_else(|| nysa_core::config::ai::EmbeddingConfig {
                    provider: None,
                    model: String::new(),
                    dimensions: None,
                    encoding_format: None,
                }),
            )
            .compaction(compaction_config);

        app_builder = app_builder.ai(ai_builder.build()?);
    } else {
        tracing::warn!("No AI configuration found. LLM features will be disabled.");
    }

    let app = app_builder.build().await?;

    let mut tools_ready_rx = app.event_bus().subscribe::<ToolsReady>();

    tracing::info!("Waiting for tools to be ready...");
    let ready = tools_ready_rx.recv().await?;
    tracing::info!(
        "Tools ready from extension '{}' (reported {} tools)",
        ready.extension_name,
        ready.tool_count
    );

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
