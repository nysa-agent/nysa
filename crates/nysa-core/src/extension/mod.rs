pub mod base;
pub mod context;
pub mod event;
pub mod factory;
pub mod manager;

pub use base::{BackgroundTask, BoxFuture, Extension, ExtensionConfig, ExtensionDef, ExtensionError};
pub use context::ExtensionContext;
pub use event::{Event, EventBus, MessageReceived, MessageSource, MessageTarget, MessageToSend, SharedEventBus};
pub use factory::{ExtensionFactoryRegistry, ExtensionFactoryRegistryBuilder};
pub use manager::{ExtensionManager, ExtensionManagerBuilder};
