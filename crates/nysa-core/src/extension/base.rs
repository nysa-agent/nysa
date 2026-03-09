use std::any::Any;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use futures::FutureExt;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::tool::ToolRegistry;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Error)]
pub enum ExtensionError {
    #[error("Extension '{0}' failed to start: {1}")]
    StartFailed(String, String),
    
    #[error("Extension '{0}' failed to stop: {1}")]
    StopFailed(String, String),
    
    #[error("Extension '{0}' timed out during {1}")]
    Timeout(String, String),
    
    #[error("Extension not found: {0}")]
    NotFound(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("{0}")]
    Custom(String),
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
    
    fn register_tools(&self, _registry: &mut ToolRegistry) {}
    
    async fn on_start(&self) -> Result<(), ExtensionError> {
        Ok(())
    }
    
    async fn on_stop(&self) -> Result<(), ExtensionError> {
        Ok(())
    }
    
    fn background_task(&self) -> Option<BackgroundTask> {
        None
    }
    
    fn as_any(&self) -> &dyn Any;
}

pub trait ExtensionConfig: Send + Sync + 'static {
    fn extension_name(&self) -> &'static str;
}

pub trait ExtensionDef: Send + Sync + 'static + Sized {
    type Config: DeserializeOwned + Send + Sync + 'static;
    
    fn extension_name() -> &'static str;
    
    fn extension_description() -> Option<&'static str> {
        None
    }
    
    fn create(config: Self::Config) -> Self;
}
