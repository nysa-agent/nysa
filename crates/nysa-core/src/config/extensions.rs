use std::any::{Any, TypeId};
use std::collections::HashMap;

pub struct ExtensionConfigRegistry {
    configs: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl ExtensionConfigRegistry {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
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
