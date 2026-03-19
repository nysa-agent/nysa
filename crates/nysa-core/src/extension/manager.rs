use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::extension::base::{BackgroundTask, Extension, ExtensionError, RestartPolicy};
use crate::extension::context::ExtensionContext;
use crate::tool::ToolRegistry;

const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

type ExtensionHolder = Arc<dyn Extension>;

struct TaskSupervisor {
    extension: ExtensionHolder,
    ctx: ExtensionContext,
    task_name: &'static str,
    restart_policy: RestartPolicy,
    restart_count: AtomicU32,
    failed: Arc<AtomicBool>,
    error: Mutex<Option<ExtensionError>>,
    stop_token: CancellationToken,
}

impl TaskSupervisor {
    fn new(
        extension: ExtensionHolder,
        ctx: ExtensionContext,
        task_name: &'static str,
        restart_policy: RestartPolicy,
        stop_token: CancellationToken,
    ) -> Self {
        Self {
            extension,
            ctx,
            task_name,
            restart_policy,
            restart_count: AtomicU32::new(0),
            failed: Arc::new(AtomicBool::new(false)),
            error: Mutex::new(None),
            stop_token,
        }
    }

    async fn run(self: Arc<Self>) {
        loop {
            if self.stop_token.is_cancelled() {
                info!(
                    "Task supervisor for '{}' received stop signal",
                    self.task_name
                );
                break;
            }

            let task = match self.extension.background_task(&self.ctx) {
                Some(t) => t,
                None => {
                    info!(
                        "Extension '{}' no longer provides background task, stopping supervisor",
                        self.extension.name()
                    );
                    break;
                }
            };

            let result = self.run_task(task).await;

            match result {
                Ok(()) => {
                    info!(
                        "Background task '{}' for extension '{}' completed normally",
                        self.task_name,
                        self.extension.name()
                    );
                    break;
                }
                Err(e) => {
                    if self.stop_token.is_cancelled() {
                        break;
                    }

                    let max_restarts = self.restart_policy.max_restarts();
                    let current_restarts = self.restart_count.load(Ordering::SeqCst);

                    if current_restarts >= max_restarts {
                        error!(
                            "Background task '{}' for extension '{}' failed (restart {}/{}): {}",
                            self.task_name,
                            self.extension.name(),
                            current_restarts,
                            max_restarts,
                            e
                        );
                        self.failed.store(true, Ordering::SeqCst);
                        *self.error.lock().await = Some(e);
                        break;
                    }

                    self.failed.store(true, Ordering::SeqCst);
                    *self.error.lock().await = Some(e.clone());
                    self.restart_count.fetch_add(1, Ordering::SeqCst);

                    if let Some(delay) = self.calculate_backoff(current_restarts) {
                        warn!(
                            "Background task '{}' for extension '{}' failed, restarting in {:?} (restart {}/{})",
                            self.task_name,
                            self.extension.name(),
                            delay,
                            current_restarts + 1,
                            max_restarts
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = self.stop_token.cancelled() => {
                                info!("Task supervisor for '{}' cancelled during backoff", self.task_name);
                                break;
                            }
                        }
                    } else {
                        tokio::task::yield_now().await;
                    }
                }
            }
        }
    }

    async fn run_task(&self, mut task: BackgroundTask) -> Result<(), ExtensionError> {
        let token = self.stop_token.clone();

        tokio::select! {
            result = &mut task.task => result,
            _ = token.cancelled() => {
                info!(
                    "Background task '{}' for extension '{}' cancelled",
                    self.task_name,
                    self.extension.name()
                );
                Ok(())
            }
        }
    }

    fn calculate_backoff(&self, attempt: u32) -> Option<Duration> {
        match &self.restart_policy {
            RestartPolicy::Never => None,
            RestartPolicy::Immediately { .. } => Some(Duration::ZERO),
            RestartPolicy::WithBackoff {
                min,
                max,
                factor,
                ..
            } => {
                let delay = min.saturating_mul(factor.pow(attempt)).min(*max);
                if delay.is_zero() {
                    Some(Duration::ZERO)
                } else {
                    Some(delay)
                }
            }
        }
    }

    fn is_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    fn get_error(&self) -> Option<ExtensionError> {
        self.error.try_lock().ok().and_then(|g| g.clone())
    }

    fn restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }
}

struct TaskHandle {
    name: &'static str,
    extension_name: String,
    handle: JoinHandle<()>,
    supervisor: Arc<TaskSupervisor>,
}

pub struct TaskStatus {
    pub name: &'static str,
    pub extension_name: String,
    pub is_running: bool,
    pub failed: bool,
    pub restart_count: u32,
    pub error: Option<ExtensionError>,
}

pub struct ExtensionManager {
    extensions: HashMap<&'static str, ExtensionHolder>,
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
        let name = extension.name();
        let holder: ExtensionHolder = Arc::new(extension) as Arc<dyn Extension>;
        self.extensions.insert(name, holder);
    }

    pub fn register_boxed(&mut self, extension: Box<dyn Extension>) {
        let holder: ExtensionHolder = Arc::from(extension);
        self.extensions.insert(holder.name(), holder);
    }

    pub fn get_by_name(&self, name: &str) -> Option<&dyn Extension> {
        self.extensions
            .get(name)
            .map(|e| e.as_ref())
    }

    pub fn all(&self) -> Vec<&dyn Extension> {
        self.extensions.values().map(|e| e.as_ref()).collect()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.extensions.keys().copied().collect()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.extensions.contains_key(name)
    }

    pub fn find(&self, predicate: impl Fn(&dyn Extension) -> bool) -> Vec<&dyn Extension> {
        self.extensions
            .values()
            .filter(|e| predicate(e.as_ref()))
            .map(|e| e.as_ref())
            .collect()
    }

    pub fn task_status(&self) -> Vec<TaskStatus> {
        self.background_tasks
            .iter()
            .map(|t| TaskStatus {
                name: t.name,
                extension_name: t.extension_name.clone(),
                is_running: !t.handle.is_finished(),
                failed: t.supervisor.is_failed(),
                restart_count: t.supervisor.restart_count(),
                error: t.supervisor.get_error(),
            })
            .collect()
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

            if extension.background_task(ctx).is_some() {
                let ext_name = name.to_string();
                let ext_name_static = extension.name();
                let restart_policy = extension.restart_policy();

                let supervisor = Arc::new(TaskSupervisor::new(
                    extension.clone(),
                    ctx.clone(),
                    ext_name_static,
                    restart_policy,
                    self.cancellation_token.clone(),
                ));

                let supervisor_handle = supervisor.clone();
                let handle = tokio::spawn(async move {
                    supervisor_handle.run().await;
                });

                self.background_tasks.push(TaskHandle {
                    name: ext_name_static,
                    extension_name: ext_name,
                    handle,
                    supervisor,
                });
            }
        }

        info!("All extensions started successfully");
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<(), Vec<ExtensionError>> {
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

        let mut errors = Vec::new();

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
                    errors.push(e);
                }
                Err(_) => {
                    let err = ExtensionError::Timeout(name.to_string(), "stop".to_string());
                    warn!(
                        "Extension '{}' force-killed after {:?} timeout",
                        name, self.shutdown_timeout
                    );
                    errors.push(err);
                }
            }
        }

        info!("All extensions stopped");

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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
