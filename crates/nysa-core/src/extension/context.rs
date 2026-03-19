use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::auth::AuthService;
use crate::compaction::CompactionManager;
use crate::config::Config;
use crate::extension::event::SharedEventBus;
use crate::llm::ConversationManager;
use crate::tool::ToolRegistry;

type SharedState = Arc<parking_lot::RwLock<HashMap<TypeId, Arc<dyn std::any::Any + Send + Sync>>>>;

pub struct ExtensionContext {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub database: Arc<DatabaseConnection>,
    pub event_bus: SharedEventBus,
    pub config: Arc<Config>,
    pub cancellation_token: CancellationToken,
    auth_service: Option<Arc<AuthService>>,
    compaction_manager: Option<Arc<CompactionManager>>,
    conversation_manager: Option<Arc<ConversationManager>>,
    state: SharedState,
}

impl ExtensionContext {
    pub fn new(
        database: DatabaseConnection,
        config: Arc<Config>,
        cancellation_token: CancellationToken,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        event_bus: SharedEventBus,
    ) -> Self {
        Self {
            tool_registry,
            database: Arc::new(database),
            event_bus,
            config,
            cancellation_token,
            auth_service: None,
            compaction_manager: None,
            conversation_manager: None,
            state: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    pub fn with_auth_service(mut self, service: AuthService) -> Self {
        self.auth_service = Some(Arc::new(service));
        self
    }

    pub fn with_compaction_manager(mut self, manager: CompactionManager) -> Self {
        self.compaction_manager = Some(Arc::new(manager));
        self
    }

    pub fn with_conversation_manager(mut self, manager: Arc<ConversationManager>) -> Self {
        self.conversation_manager = Some(manager);
        self
    }

    pub fn auth(&self) -> Option<&Arc<AuthService>> {
        self.auth_service.as_ref()
    }

    pub fn compaction(&self) -> Option<&Arc<CompactionManager>> {
        self.compaction_manager.as_ref()
    }

    pub fn conversation(&self) -> Option<&Arc<ConversationManager>> {
        self.conversation_manager.as_ref()
    }

    pub fn store<T: 'static + Send + Sync>(&self, value: T) {
        let mut state = self.state.write();
        state.insert(TypeId::of::<T>(), Arc::new(value));
    }

    pub fn get<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let state = self.state.read();
        state
            .get(&TypeId::of::<T>())
            .and_then(|v| v.clone().downcast::<Arc<T>>().ok())
            .map(Arc::unwrap_or_clone)
    }

    pub fn spawn_task<F>(&self, name: &str, future: F) -> tokio::task::JoinHandle<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let token = self.cancellation_token.clone();
        let task_name = name.to_string();
        tokio::spawn(async move {
            tokio::select! {
                _ = future => {
                    tracing::debug!("Extension task '{}' completed", task_name);
                }
                _ = token.cancelled() => {
                    tracing::debug!("Extension task '{}' cancelled", task_name);
                }
            }
        })
    }

    pub fn is_shutting_down(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }
}

impl Clone for ExtensionContext {
    fn clone(&self) -> Self {
        Self {
            tool_registry: self.tool_registry.clone(),
            database: self.database.clone(),
            event_bus: self.event_bus.clone(),
            config: self.config.clone(),
            cancellation_token: self.cancellation_token.clone(),
            auth_service: self.auth_service.clone(),
            compaction_manager: self.compaction_manager.clone(),
            conversation_manager: self.conversation_manager.clone(),
            state: Arc::clone(&self.state),
        }
    }
}
