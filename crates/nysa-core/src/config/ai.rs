use anyhow::anyhow;
use async_openai::types::{ChatCompletionRequestMessage, EncodingFormat};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};

const NYSA_REFERER: &str = "https://nysa.phrolova.moe/";
const NYSA_TITLE: &str = "Nysa";

#[derive(Clone)]
pub struct Provider {
    pub name: String,
    pub base_url: String,
    pub api_key: SecretString,
}

impl Provider {
    pub fn new(name: impl Into<String>, base_url: String, api_key: String) -> Self {
        Self {
            name: name.into(),
            base_url,
            api_key: SecretString::from(api_key),
        }
    }

    pub fn to_openai_config(&self) -> NysaOpenAiConfig {
        NysaOpenAiConfig::new(self.base_url.clone(), self.api_key.clone())
    }
}

#[derive(Clone)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_completion_tokens: Option<u32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop_sequences: Vec<String>,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            top_p: None,
            max_completion_tokens: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop_sequences: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct ChatConfig {
    pub provider: Option<Provider>,
    pub model: String,
    pub options: ChatOptions,
}

impl ChatConfig {
    pub fn provider_or_default<'a>(&'a self, default: &'a Provider) -> &'a Provider {
        self.provider.as_ref().unwrap_or(default)
    }
}

#[derive(Clone)]
pub struct EmbeddingConfig {
    pub provider: Option<Provider>,
    pub model: String,
    pub dimensions: Option<u32>,
    pub encoding_format: Option<EncodingFormat>,
}

impl EmbeddingConfig {
    pub fn provider_or_default<'a>(&'a self, default: &'a Provider) -> &'a Provider {
        self.provider.as_ref().unwrap_or(default)
    }

    pub fn create_client_config<'a>(&self, default_provider: &'a Provider) -> impl async_openai::config::Config + 'a {
        self.provider_or_default(default_provider).to_openai_config()
    }
}

#[derive(Clone)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub auto_threshold: f32,
    pub max_messages_to_summarize: usize,
    pub preserve_recent: usize,
    pub provider: Option<Provider>,
    pub summary_model: Option<String>,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_threshold: 0.75,
            max_messages_to_summarize: 50,
            preserve_recent: 10,
            provider: None,
            summary_model: None,
        }
    }
}

impl CompactionConfig {
    pub fn provider_or_default<'a>(&'a self, default: &'a Provider) -> &'a Provider {
        self.provider.as_ref().unwrap_or(default)
    }

    pub fn summary_model_or_default<'a>(&'a self, default: &'a str) -> &'a str {
        self.summary_model.as_deref().unwrap_or(default)
    }
}

#[derive(Clone)]
pub struct AiConfig {
    pub provider: Provider,
    pub chat: ChatConfig,
    pub embedding: EmbeddingConfig,
    pub compaction: CompactionConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        let default_provider = Provider::new("default", String::new(), String::new());
        Self {
            provider: default_provider.clone(),
            chat: ChatConfig {
                provider: None,
                model: String::new(),
                options: ChatOptions::default(),
            },
            embedding: EmbeddingConfig {
                provider: None,
                model: String::new(),
                dimensions: None,
                encoding_format: None,
            },
            compaction: CompactionConfig::default(),
        }
    }
}

impl AiConfig {
    pub fn chat_provider(&self) -> &Provider {
        self.chat.provider_or_default(&self.provider)
    }

    pub fn embedding_provider(&self) -> &Provider {
        self.embedding.provider_or_default(&self.provider)
    }

    pub fn compaction_provider(&self) -> &Provider {
        self.compaction.provider_or_default(&self.provider)
    }

    pub fn chat_model_or_default(&self) -> &str {
        if self.chat.model.is_empty() {
            "gpt-4o"
        } else {
            &self.chat.model
        }
    }

    pub fn embedding_model_or_default(&self) -> String {
        if self.embedding.model.is_empty() {
            "text-embedding-3-small".to_string()
        } else {
            self.embedding.model.clone()
        }
    }

    pub fn compaction_model_or_default(&self) -> &str {
        self.compaction
            .summary_model_or_default(self.chat_model_or_default())
    }
}

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<crate::llm::types::LlmResponse, crate::llm::types::LlmError>;
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(
        &self,
        model: &str,
        input: async_openai::types::EmbeddingInput,
    ) -> Result<Vec<Vec<f32>>, crate::llm::types::LlmError>;
}

#[async_trait::async_trait]
pub trait SummarizationProvider: Send + Sync {
    async fn summarize(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<String, crate::llm::types::LlmError>;
}

#[derive(Clone)]
pub struct NysaOpenAiConfig {
    base_url: String,
    api_key: SecretString,
}

impl NysaOpenAiConfig {
    pub fn new(base_url: String, api_key: SecretString) -> Self {
        Self { base_url, api_key }
    }
}

impl async_openai::config::Config for NysaOpenAiConfig {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", self.api_key.expose_secret())
                .parse()
                .unwrap(),
        );
        headers.insert("HTTP-Referer", NYSA_REFERER.parse().unwrap());
        headers.insert("X-Title", NYSA_TITLE.parse().unwrap());
        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn api_base(&self) -> &str {
        &self.base_url
    }

    fn api_key(&self) -> &SecretString {
        &self.api_key
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}

pub struct AiConfigBuilder {
    provider: Option<Provider>,
    chat: Option<ChatConfig>,
    embedding: Option<EmbeddingConfig>,
    compaction: Option<CompactionConfig>,
}

impl AiConfigBuilder {
    pub fn new() -> Self {
        Self {
            provider: None,
            chat: None,
            embedding: None,
            compaction: None,
        }
    }

    pub fn provider(mut self, provider: Provider) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn provider_with_defaults(mut self, base_url: String, api_key: String) -> Self {
        self.provider = Some(Provider::new("default", base_url, api_key));
        self
    }

    pub fn chat(mut self, chat: ChatConfig) -> Self {
        self.chat = Some(chat);
        self
    }

    pub fn embedding(mut self, embedding: EmbeddingConfig) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn compaction(mut self, compaction: CompactionConfig) -> Self {
        self.compaction = Some(compaction);
        self
    }

    pub fn build(self) -> anyhow::Result<AiConfig> {
        let provider = self
            .provider
            .ok_or_else(|| anyhow!("provider is required"))?;

        let chat = self.chat.ok_or_else(|| anyhow!("chat config is required"))?;
        let mut embedding = self.embedding.unwrap_or(EmbeddingConfig {
            provider: None,
            model: String::new(),
            dimensions: None,
            encoding_format: None,
        });
        if embedding.provider.is_none() {
            embedding.provider = Some(provider.clone());
        }
        let mut compaction = self.compaction.unwrap_or_default();
        if compaction.provider.is_none() {
            compaction.provider = Some(provider.clone());
        }

        Ok(AiConfig {
            provider,
            chat,
            embedding,
            compaction,
        })
    }
}

impl Default for AiConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EmbeddingConfigBuilder {
    provider: Option<Provider>,
    model: Option<String>,
    dimensions: Option<u32>,
    encoding_format: Option<EncodingFormat>,
}

impl EmbeddingConfigBuilder {
    pub fn new() -> Self {
        Self {
            provider: None,
            model: None,
            dimensions: None,
            encoding_format: None,
        }
    }

    pub fn provider(mut self, provider: Provider) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn provider_with_defaults(mut self, base_url: String, api_key: String) -> Self {
        self.provider = Some(Provider::new("default", base_url, api_key));
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn dimensions(mut self, dims: u32) -> Self {
        self.dimensions = Some(dims);
        self
    }

    pub fn encoding_format(mut self, format: EncodingFormat) -> Self {
        self.encoding_format = Some(format);
        self
    }

    pub fn build(self) -> anyhow::Result<EmbeddingConfig> {
        let model = self.model.ok_or_else(|| anyhow!("model is required"))?;

        Ok(EmbeddingConfig {
            provider: self.provider,
            model,
            dimensions: self.dimensions,
            encoding_format: self.encoding_format,
        })
    }
}

impl Default for EmbeddingConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
