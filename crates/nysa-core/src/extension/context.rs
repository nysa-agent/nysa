use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::extension::event::{EventBus, SharedEventBus};
use crate::tool::ToolRegistry;

pub struct ExtensionContext {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub database: Arc<DatabaseConnection>,
    pub event_bus: SharedEventBus,
    pub config: Arc<Config>,
    pub cancellation_token: CancellationToken,
    state: parking_lot::RwLock<HashMap<TypeId, Arc<dyn std::any::Any + Send + Sync>>>,
}

impl ExtensionContext {
    pub fn new(
        database: DatabaseConnection,
        config: Config,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            tool_registry: Arc::new(RwLock::new(ToolRegistry::new())),
            database: Arc::new(database),
            event_bus: Arc::new(EventBus::new()),
            config: Arc::new(config),
            cancellation_token,
            state: parking_lot::RwLock::new(HashMap::new()),
        }
    }
    
    pub fn store<T: 'static + Send + Sync>(&self, value: T) {
        let mut state = self.state.write();
        state.insert(TypeId::of::<T>(), Arc::new(value));
    }
    
    pub fn get<T: 'static + Clone + Send + Sync>(&self) -> Option<T> {
        let state = self.state.read();
        state
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }
    
    pub fn spawn_task<F>(&self, _name: &str, future: F) -> tokio::task::JoinHandle<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let token = self.cancellation_token.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = future => {}
                _ = token.cancelled() => {}
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
            state: parking_lot::RwLock::new(HashMap::new()),
        }
    }
}
