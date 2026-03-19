pub mod ai;
pub mod extensions;

pub use ai::{
    AiConfig, AiConfigBuilder, ChatOptions, ChatProvider, CompactionConfig,
    EmbeddingConfig, EmbeddingConfigBuilder, NysaOpenAiConfig, Provider, SummarizationProvider,
};
pub use extensions::{ExtensionConfig, ExtensionConfigRegistry};

pub struct Config {
    pub ai: Option<AiConfig>,
    pub extensions: ExtensionConfigRegistry,
}

impl Config {
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai: None,
            extensions: ExtensionConfigRegistry::new(),
        }
    }
}

pub struct ConfigBuilder {
    ai: Option<AiConfig>,
    extensions: ExtensionConfigRegistry,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            ai: None,
            extensions: ExtensionConfigRegistry::new(),
        }
    }

    pub fn ai(mut self, config: AiConfig) -> Self {
        self.ai = Some(config);
        self
    }

    pub fn extension<T: extensions::ExtensionConfig>(mut self, config: T) -> Self {
        self.extensions.register(config);
        self
    }

    pub fn build(self) -> Config {
        Config {
            ai: self.ai,
            extensions: self.extensions,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
