use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use serde::de::DeserializeOwned;

use crate::tool::ToolRegistry;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, Default)]
pub enum RestartPolicy {
    #[default]
    Never,
    Immediately {
        max_restarts: u32,
    },
    WithBackoff {
        min: Duration,
        max: Duration,
        factor: u32,
        max_restarts: u32,
    },
}

impl RestartPolicy {
    pub fn never() -> Self {
        RestartPolicy::Never
    }

    pub fn immediately(max_restarts: u32) -> Self {
        RestartPolicy::Immediately { max_restarts }
    }

    pub fn with_backoff(min: Duration, max: Duration, factor: u32, max_restarts: u32) -> Self {
        RestartPolicy::WithBackoff {
            min,
            max,
            factor,
            max_restarts,
        }
    }

    pub fn max_restarts(&self) -> u32 {
        match self {
            RestartPolicy::Never => 0,
            RestartPolicy::Immediately { max_restarts } => *max_restarts,
            RestartPolicy::WithBackoff { max_restarts, .. } => *max_restarts,
        }
    }

    pub fn is_never(&self) -> bool {
        matches!(self, RestartPolicy::Never)
    }
}

#[derive(Debug)]
pub enum ExtensionError {
    StartFailed {
        name: String,
        reason: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    StopFailed {
        name: String,
        reason: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    Timeout(String, String),

    NotFound(String),

    Io(std::io::Error),

    Custom(String),
}

impl Clone for ExtensionError {
    fn clone(&self) -> Self {
        match self {
            ExtensionError::StartFailed {
                name,
                reason,
                source,
            } => ExtensionError::StartFailed {
                name: name.clone(),
                reason: reason.clone(),
                source: source.as_ref().map(|s| {
                    Box::new(std::io::Error::other(s.to_string())) as Box<dyn std::error::Error + Send + Sync>
                }),
            },
            ExtensionError::StopFailed {
                name,
                reason,
                source,
            } => ExtensionError::StopFailed {
                name: name.clone(),
                reason: reason.clone(),
                source: source.as_ref().map(|s| {
                    Box::new(std::io::Error::other(s.to_string())) as Box<dyn std::error::Error + Send + Sync>
                }),
            },
            ExtensionError::Timeout(a, b) => ExtensionError::Timeout(a.clone(), b.clone()),
            ExtensionError::NotFound(a) => ExtensionError::NotFound(a.clone()),
            ExtensionError::Io(e) => {
                ExtensionError::Io(std::io::Error::new(e.kind(), e.to_string()))
            }
            ExtensionError::Custom(s) => ExtensionError::Custom(s.clone()),
        }
    }
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtensionError::StartFailed { name, reason, .. } => {
                write!(f, "Extension '{}' failed to start: {}", name, reason)
            }
            ExtensionError::StopFailed { name, reason, .. } => {
                write!(f, "Extension '{}' failed to stop: {}", name, reason)
            }
            ExtensionError::Timeout(name, op) => {
                write!(f, "Extension '{}' timed out during {}", name, op)
            }
            ExtensionError::NotFound(name) => write!(f, "Extension not found: {}", name),
            ExtensionError::Io(e) => write!(f, "IO error: {}", e),
            ExtensionError::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for ExtensionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExtensionError::StartFailed { source, .. } => source.as_ref().map(|e| e.as_ref() as _),
            ExtensionError::StopFailed { source, .. } => source.as_ref().map(|e| e.as_ref() as _),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ExtensionError {
    fn from(e: std::io::Error) -> Self {
        ExtensionError::Io(e)
    }
}

impl From<&str> for ExtensionError {
    fn from(msg: &str) -> Self {
        ExtensionError::Custom(msg.to_string())
    }
}

impl From<String> for ExtensionError {
    fn from(msg: String) -> Self {
        ExtensionError::Custom(msg)
    }
}

impl ExtensionError {
    pub fn start_failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        ExtensionError::StartFailed {
            name: name.into(),
            reason: reason.into(),
            source: None,
        }
    }

    pub fn start_failed_with_source(
        name: impl Into<String>,
        reason: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        ExtensionError::StartFailed {
            name: name.into(),
            reason: reason.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn stop_failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        ExtensionError::StopFailed {
            name: name.into(),
            reason: reason.into(),
            source: None,
        }
    }

    pub fn stop_failed_with_source(
        name: impl Into<String>,
        reason: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        ExtensionError::StopFailed {
            name: name.into(),
            reason: reason.into(),
            source: Some(Box::new(source)),
        }
    }
}

pub struct BackgroundTask {
    pub name: &'static str,
    pub task: BoxFuture<'static, Result<(), ExtensionError>>,
}

impl BackgroundTask {
    pub fn new<F>(name: &'static str, task: F) -> Self
    where
        F: Future<Output = Result<(), ExtensionError>> + Send + 'static,
    {
        Self {
            name,
            task: task.boxed(),
        }
    }
}

#[async_trait]
pub trait Extension: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn description(&self) -> Option<&'static str> {
        None
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::default()
    }

    fn register_tools(&self, _registry: &mut ToolRegistry) {}

    async fn on_start(&self) -> Result<(), ExtensionError> {
        Ok(())
    }

    async fn on_stop(&self) -> Result<(), ExtensionError> {
        Ok(())
    }

    fn background_task(
        &self,
        _ctx: &crate::extension::context::ExtensionContext,
    ) -> Option<BackgroundTask> {
        None
    }

    fn as_any(&self) -> &dyn Any;

    fn prompt_provider(&self) -> Option<&dyn crate::prompt::PromptProvider> {
        None
    }
}

pub struct ExtensionDescription {
    pub name: &'static str,
    pub description: Option<&'static str>,
    pub version: Option<&'static str>,
    pub author: Option<&'static str>,
}

impl ExtensionDescription {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            description: None,
            version: None,
            author: None,
        }
    }

    pub fn description(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }

    pub fn version(mut self, version: &'static str) -> Self {
        self.version = Some(version);
        self
    }

    pub fn author(mut self, author: &'static str) -> Self {
        self.author = Some(author);
        self
    }
}

pub trait ExtensionDef: Send + Sync + 'static + Sized {
    type Config: DeserializeOwned + Send + Sync + 'static;

    fn extension_name() -> &'static str;

    fn extension_description() -> Option<&'static str> {
        None
    }

    fn create(config: Self::Config) -> Self;
}
