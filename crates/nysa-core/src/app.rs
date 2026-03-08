use crate::config::{AiConfig, Config, ExtensionConfigRegistry};
use sea_orm::DatabaseConnection;
use tracing::{Subscriber, error, info};
use tracing_subscriber::{
    EnvFilter,
    fmt::{FormatEvent, FormatFields},
    registry::LookupSpan,
};

pub struct App {
    pub database: DatabaseConnection,
    pub config: Config,
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
            .with_env_filter(EnvFilter::new("info,sqlx::query=warn"))
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

    pub fn extensions(&self) -> &ExtensionConfigRegistry {
        &self.config.extensions
    }
}

pub struct AppBuilder {
    database: DatabaseConnection,
    config: Config,
}

impl AppBuilder {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            database: db,
            config: Config::default(),
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

    pub fn extension<T: crate::config::ExtensionConfig>(mut self, config: T) -> Self {
        self.config.extensions.register(config);
        self
    }

    pub async fn build(self) -> anyhow::Result<App> {
        App::init_logging();
        App::sync_database(&self.database).await?;

        Ok(App {
            database: self.database,
            config: self.config,
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
