use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::extension::base::{Extension, ExtensionError};
use crate::extension::context::ExtensionContext;
use crate::tool::ToolRegistry;

const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

type ExtensionHolder = Arc<dyn Extension>;

struct TaskHandle {
    name: &'static str,
    extension_name: String,
    handle: JoinHandle<()>,
}

pub struct ExtensionManager {
    extensions: HashMap<TypeId, ExtensionHolder>,
    background_tasks: Vec<TaskHandle>,
    shutdown_timeout: Duration,
    cancellation_token: CancellationToken,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
            background_tasks: Vec::new(),
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
            cancellation_token: CancellationToken::new(),
        }
    }

    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub fn register<E: Extension>(&mut self, extension: E) {
        let type_id = TypeId::of::<E>();
        let holder: ExtensionHolder = Arc::new(extension) as Arc<dyn Extension>;
        self.extensions.insert(type_id, holder);
    }

    pub fn get<E: Extension>(&self) -> Option<&E> {
        let type_id = TypeId::of::<E>();
        self.extensions
            .get(&type_id)
            .and_then(|holder| holder.as_any().downcast_ref::<E>())
    }

    pub fn all(&self) -> Vec<&dyn Extension> {
        self.extensions.values().map(|e| e.as_ref()).collect()
    }

    pub fn len(&self) -> usize {
        self.extensions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    pub async fn register_tools(&self, registry: &mut ToolRegistry) {
        for extension in self.extensions.values() {
            extension.register_tools(registry);
        }
    }

    pub async fn start_all(&mut self, ctx: &ExtensionContext) -> Result<(), ExtensionError> {
        if self.extensions.is_empty() {
            info!("No extensions to start");
            return Ok(());
        }

        info!("Starting {} extension(s)...", self.extensions.len());

        for extension in self.extensions.values() {
            let name = extension.name();
            let description = extension.description();

            if let Some(desc) = description {
                info!("{} - {}", name, desc);
            } else {
                info!("{}", name);
            }

            let start = Instant::now();
            match extension.on_start().await {
                Ok(()) => {
                    let elapsed = start.elapsed();
                    if elapsed > Duration::from_millis(100) {
                        info!("Extension '{}' started in {:?}", name, elapsed);
                    }
                }
                Err(e) => {
                    error!("Failed to start extension '{}': {}", name, e);
                    return Err(e);
                }
            }

            if let Some(task) = extension.background_task(ctx) {
                let token = self.cancellation_token.clone();
                let ext_name = name.to_string();
                let task_name = task.name;

                let handle = tokio::spawn(async move {
                    let result = tokio::select! {
                        result = task.task => result,
                        _ = token.cancelled() => {
                            info!("Background task '{}' for extension '{}' received shutdown signal", task_name, ext_name);
                            return;
                        }
                    };

                    if let Err(e) = result {
                        error!(
                            "Background task '{}' for extension '{}' failed: {}",
                            task_name, ext_name, e
                        );
                    }
                });

                self.background_tasks.push(TaskHandle {
                    name: task.name,
                    extension_name: name.to_string(),
                    handle,
                });
            }
        }

        info!("All extensions started successfully");
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<(), ExtensionError> {
        if self.extensions.is_empty() {
            return Ok(());
        }

        info!("Stopping {} extension(s)...", self.extensions.len());

        self.cancellation_token.cancel();

        let tasks = std::mem::take(&mut self.background_tasks);
        for task in tasks {
            let start = Instant::now();

            tokio::select! {
                _ = task.handle => {
                    let elapsed = start.elapsed();
                    info!("Extension '{}' task '{}' stopped in {:?}",
                          task.extension_name, task.name, elapsed);
                }
                _ = tokio::time::sleep(self.shutdown_timeout) => {
                    warn!("Extension '{}' task '{}' force-killed after {:?} timeout",
                          task.extension_name, task.name, self.shutdown_timeout);
                }
            }
        }

        for extension in self.extensions.values() {
            let name = extension.name();
            let start = Instant::now();

            match tokio::time::timeout(self.shutdown_timeout, extension.on_stop()).await {
                Ok(Ok(())) => {
                    let elapsed = start.elapsed();
                    if elapsed > Duration::from_millis(100) {
                        info!("Extension '{}' stopped in {:?}", name, elapsed);
                    }
                }
                Ok(Err(e)) => {
                    error!("Extension '{}' failed to stop: {}", name, e);
                }
                Err(_) => {
                    warn!(
                        "Extension '{}' force-killed after {:?} timeout",
                        name, self.shutdown_timeout
                    );
                }
            }
        }

        info!("All extensions stopped");
        Ok(())
    }
}

impl Default for ExtensionManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ExtensionManagerBuilder {
    manager: ExtensionManager,
}

impl ExtensionManagerBuilder {
    pub fn new() -> Self {
        Self {
            manager: ExtensionManager::new(),
        }
    }

    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.manager.shutdown_timeout = timeout;
        self
    }

    pub fn extension<E: Extension>(mut self, extension: E) -> Self {
        self.manager.register(extension);
        self
    }

    pub fn build(self) -> ExtensionManager {
        self.manager
    }
}

impl Default for ExtensionManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
