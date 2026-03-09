use std::any::{Any, TypeId};
use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub struct ExtensionConfigRegistry {
    configs: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    raw_configs: HashMap<String, toml::Value>,
}

impl ExtensionConfigRegistry {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            raw_configs: HashMap::new(),
        }
    }

    pub fn register<T: 'static + Send + Sync>(&mut self, config: T) {
        self.configs.insert(TypeId::of::<T>(), Box::new(config));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.configs
            .get(&TypeId::of::<T>())
            .and_then(|c| c.downcast_ref::<T>())
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.configs
            .get_mut(&TypeId::of::<T>())
            .and_then(|c| c.downcast_mut::<T>())
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.configs
            .remove(&TypeId::of::<T>())
            .and_then(|c| c.downcast::<T>().ok())
            .map(|b| *b)
    }

    pub fn contains<T: 'static>(&self) -> bool {
        self.configs.contains_key(&TypeId::of::<T>())
    }

    pub fn len(&self) -> usize {
        self.configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    pub fn register_raw(&mut self, name: impl Into<String>, config: toml::Value) {
        self.raw_configs.insert(name.into(), config);
    }

    pub fn get_raw(&self, name: &str) -> Option<&toml::Value> {
        self.raw_configs.get(name)
    }

    pub fn deserialize<T: DeserializeOwned + ExtensionConfig + Clone>(
        &mut self,
        name: &str,
    ) -> Option<T> {
        let raw = self.raw_configs.get(name)?;
        let config: T = raw.clone().try_into().ok()?;
        self.register(config.clone());
        Some(config)
    }

    pub fn raw_config_names(&self) -> Vec<&str> {
        self.raw_configs.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ExtensionConfigRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ExtensionConfigRegistry {
    fn clone(&self) -> Self {
        panic!("ExtensionConfigRegistry cannot be cloned - use Arc if sharing is needed");
    }
}

pub trait ExtensionConfig: Send + Sync + 'static {
    fn extension_name(&self) -> &'static str;
}

impl ExtensionConfigRegistry {
    pub fn register_extension<T: ExtensionConfig>(&mut self, config: T) {
        self.register(config);
    }

    pub fn get_extension<T: ExtensionConfig>(&self) -> Option<&T> {
        self.get::<T>()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionsToml {
    #[serde(flatten)]
    pub extensions: HashMap<String, toml::Value>,
}

impl ExtensionsToml {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: impl Into<String>, config: toml::Value) {
        self.extensions.insert(name.into(), config);
    }

    pub fn get(&self, name: &str) -> Option<&toml::Value> {
        self.extensions.get(name)
    }

    pub fn into_registry(self) -> ExtensionConfigRegistry {
        let mut registry = ExtensionConfigRegistry::new();
        for (name, config) in self.extensions {
            registry.register_raw(name, config);
        }
        registry
    }
}

pub fn load_extensions_from_toml(toml_str: &str) -> Result<ExtensionsToml, toml::de::Error> {
    toml::from_str(toml_str)
}

pub fn load_extensions_from_file(
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<ExtensionsToml> {
    let content = std::fs::read_to_string(path)?;
    load_extensions_from_toml(&content).map_err(std::io::Error::other)
}
