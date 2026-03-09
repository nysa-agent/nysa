use std::collections::HashMap;

use serde_json::Value;
use tracing::warn;

use crate::extension::base::{Extension, ExtensionDef};

type FactoryFn = Box<dyn Fn(Value) -> Option<Box<dyn Extension>> + Send + Sync>;

struct ExtensionInfo {
    factory: FactoryFn,
    description: Option<&'static str>,
}

pub struct ExtensionFactoryRegistry {
    factories: HashMap<&'static str, ExtensionInfo>,
}

impl ExtensionFactoryRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register<E>(&mut self)
    where
        E: ExtensionDef + Extension + 'static,
    {
        let description = E::extension_description();
        let factory: FactoryFn = Box::new(|config: Value| {
            let parsed: E::Config = serde_json::from_value(config).ok()?;
            Some(Box::new(E::create(parsed)))
        });

        self.factories.insert(
            E::extension_name(),
            ExtensionInfo {
                factory,
                description,
            },
        );
    }

    pub fn create(&self, name: &str, config: Value) -> Option<Box<dyn Extension>> {
        let info = self.factories.get(name)?;
        (info.factory)(config)
    }

    pub fn create_or_warn(&self, name: &str, config: Value) -> Option<Box<dyn Extension>> {
        if !self.factories.contains_key(name) {
            warn!("Extension '{}' is not registered, skipping", name);
            return None;
        }

        match self.create(name, config) {
            Some(ext) => Some(ext),
            None => {
                warn!("Failed to create extension '{}' with provided config", name);
                None
            }
        }
    }

    pub fn known_extensions(&self) -> Vec<&'static str> {
        self.factories.keys().copied().collect()
    }

    pub fn extension_descriptions(&self) -> Vec<(&'static str, Option<&'static str>)> {
        self.factories
            .iter()
            .map(|(name, info)| (*name, info.description))
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.factories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

impl Default for ExtensionFactoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ExtensionFactoryRegistryBuilder {
    registry: ExtensionFactoryRegistry,
}

impl ExtensionFactoryRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ExtensionFactoryRegistry::new(),
        }
    }

    pub fn register<E>(mut self) -> Self
    where
        E: ExtensionDef + Extension + 'static,
    {
        self.registry.register::<E>();
        self
    }

    pub fn build(self) -> ExtensionFactoryRegistry {
        self.registry
    }
}

impl Default for ExtensionFactoryRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
