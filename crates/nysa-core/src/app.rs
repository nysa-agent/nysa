use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;
use tracing::{Subscriber, error, info};
use tracing_subscriber::{
    EnvFilter,
    fmt::{FormatEvent, FormatFields},
    registry::LookupSpan,
};

use crate::auth::AuthService;
use crate::compaction::CompactionManager;
use crate::config::{AiConfig, Config};
use crate::extension::{EventBus, Extension, ExtensionContext, ExtensionFactoryRegistry, ExtensionManager};
use crate::llm::{ConversationManager, LlmClient, LlmConfig, MessageHistoryService};
use crate::tool::{ToolDefinition, ToolExecutor, ToolHandler, ToolRegistry};

const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct App {
    pub database: DatabaseConnection,
    pub config: Arc<Config>,
    extensions: ExtensionManager,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    tool_executor: Arc<ToolExecutor>,
    event_bus: Arc<EventBus>,
    conversation_manager: Option<Arc<ConversationManager>>,
}

impl App {
    pub fn builder(db: DatabaseConnection) -> AppBuilder {
        AppBuilder::new(db)
    }

    pub async fn init(db: DatabaseConnection) -> anyhow::Result<Self> {
        Self::builder(db).build().await
    }

    fn init_logging() {
        tracing_subscriber::fmt()
            .event_format(LogFormatter)
            .with_env_filter(EnvFilter::new("info,sqlx::query=warn,nysa_discord=debug"))
            .init();

        info!("initialized logging");
    }

    async fn sync_database(db: &DatabaseConnection) -> anyhow::Result<()> {
        match db
            .get_schema_registry(module_path!().split("::").next().unwrap())
            .sync(db)
            .await
        {
            Ok(_) => info!("synced database"),
            Err(e) => {
                error!("failed to sync database");
                return Err(e.into());
            }
        }

        Ok(())
    }

    pub fn ai(&self) -> Option<&AiConfig> {
        self.config.ai.as_ref()
    }

    pub fn extensions(&self) -> &ExtensionManager {
        &self.extensions
    }

    pub fn tool_registry(&self) -> Arc<RwLock<ToolRegistry>> {
        self.tool_registry.clone()
    }

    pub fn tool_executor(&self) -> Arc<ToolExecutor> {
        self.tool_executor.clone()
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    /// Get the conversation manager for LLM interactions
    /// Returns None if AI is not configured
    pub fn conversation_manager(&self) -> Option<Arc<ConversationManager>> {
        self.conversation_manager.clone()
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Nysa is running...");
        Ok(())
    }

    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        info!("Shutting down Nysa...");
        if let Err(errors) = self.extensions.stop_all().await {
            for err in &errors {
                error!("Extension shutdown error: {}", err);
            }
            anyhow::bail!("{} extension(s) failed to stop cleanly", errors.len());
        }
        info!("Shutdown complete");
        Ok(())
    }
}

pub struct AppBuilder {
    database: DatabaseConnection,
    config: Config,
    extensions: ExtensionManager,
    tool_registry: ToolRegistry,
    factory_registry: Option<ExtensionFactoryRegistry>,
    shutdown_timeout: Duration,
}

impl AppBuilder {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            database: db,
            config: Config::default(),
            extensions: ExtensionManager::new(),
            tool_registry: ToolRegistry::new(),
            factory_registry: None,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn ai(mut self, ai_config: AiConfig) -> Self {
        self.config.ai = Some(ai_config);
        self
    }

    pub fn extension<E: Extension>(mut self, extension: E) -> Self {
        self.extensions.register(extension);
        self
    }

    pub fn factory_registry(mut self, registry: ExtensionFactoryRegistry) -> Self {
        self.factory_registry = Some(registry);
        self
    }

    pub fn extension_from_config(
        mut self,
        name: &str,
        config: serde_json::Value,
    ) -> Self {
        if let Some(ref factory) = self.factory_registry {
            if let Some(ext) = factory.create(name, config) {
                self.extensions.register_boxed(ext);
            }
        }
        self
    }

    pub fn tool<H: ToolHandler + 'static>(
        mut self,
        definition: ToolDefinition,
        handler: H,
    ) -> Self {
        self.tool_registry.register(definition, handler);
        self
    }

    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    pub async fn build(mut self) -> anyhow::Result<App> {
        App::init_logging();
        App::sync_database(&self.database).await?;

        let event_bus = Arc::new(EventBus::new());

        self.extensions
            .register_tools(&mut self.tool_registry)
            .await;

        let tool_registry = Arc::new(RwLock::new(self.tool_registry));
        let tool_executor = Arc::new(ToolExecutor::new(Arc::clone(&tool_registry)));

        let config = Arc::new(self.config);

        let auth_service = AuthService::new(self.database.clone());
        let compaction_manager = CompactionManager::new(self.database.clone());

        // Build conversation manager if AI is configured
        let conversation_manager = if let Some(ref ai_config) = config.ai {
            info!("Building conversation manager with AI configuration");

            let llm_client = Arc::new(LlmClient::new(&ai_config.chat));
            let history_service = Arc::new(MessageHistoryService::new(self.database.clone()));
            let compaction_service = compaction_manager.service();

            let llm_config = LlmConfig {
                max_context_tokens: 120_000,
                compaction_threshold: 0.8,
                max_tool_iterations: 10,
                default_mode: crate::llm::ResponseMode::Batch,
                system_prompt_override: None,
            };

            Some(Arc::new(ConversationManager::new(
                llm_client,
                history_service,
                Some(Arc::clone(&tool_executor)),
                compaction_service,
                llm_config,
            )))
        } else {
            info!("No AI configuration found, conversation manager disabled");
            None
        };

        let ctx = ExtensionContext::new(
            self.database.clone(),
            config.clone(),
            self.extensions.cancellation_token(),
            Arc::clone(&tool_registry),
            Arc::clone(&event_bus),
        )
        .with_auth_service(auth_service)
        .with_compaction_manager(compaction_manager);

        let ctx = if let Some(ref manager) = conversation_manager {
            ctx.with_conversation_manager(Arc::clone(manager))
        } else {
            ctx
        };

        self.extensions.start_all(&ctx).await?;

        info!("Nysa initialized successfully");

        Ok(App {
            database: self.database,
            config,
            extensions: self.extensions,
            tool_registry,
            tool_executor,
            event_bus,
            conversation_manager,
        })
    }
}

struct LogFormatter;

impl<S, N> FormatEvent<S, N> for LogFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        let timer = tracing_subscriber::fmt::time::ChronoLocal::new("%H:%M:%S".to_string());
        use tracing_subscriber::fmt::time::FormatTime;
        timer.format_time(&mut writer)?;

        let level = meta.level();
        let level_color = match *level {
            tracing::Level::ERROR => "\x1b[31m",
            tracing::Level::WARN => "\x1b[33m",
            tracing::Level::INFO => "\x1b[32m",
            tracing::Level::DEBUG => "\x1b[34m",
            tracing::Level::TRACE => "\x1b[35m",
        };
        write!(writer, " {level_color}{level:>5}\x1b[0m")?;

        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        write!(writer, " [\x1b[2m{thread_name}\x1b[0m]")?;

        write!(writer, " ")?;
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}
