use anyhow::anyhow;
use async_openai::types::{EncodingFormat, Stop};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};

const NYSA_REFERER: &str = "https://nysa.phrolova.moe/";
const NYSA_TITLE: &str = "Nysa";

#[derive(Clone)]
pub struct AiConfig {
    pub chat: ChatConfig,
    pub embedding: EmbeddingConfig,
}

#[derive(Clone)]
pub struct ChatConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub options: ChatOptions,
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

impl ChatConfig {
    pub fn to_openai_request(
        &self,
        messages: Vec<async_openai::types::ChatCompletionRequestMessage>,
    ) -> async_openai::types::CreateChatCompletionRequest {
        self.to_openai_request_with_tools(messages, None)
    }

    pub fn to_openai_request_with_tools(
        &self,
        messages: Vec<async_openai::types::ChatCompletionRequestMessage>,
        tools: Option<Vec<async_openai::types::ChatCompletionTool>>,
    ) -> async_openai::types::CreateChatCompletionRequest {
        use async_openai::types::CreateChatCompletionRequest;

        let mut request = CreateChatCompletionRequest {
            model: self.model.clone(),
            messages,
            ..Default::default()
        };

        if let Some(temp) = self.options.temperature {
            request.temperature = Some(temp);
        }
        if let Some(top_p) = self.options.top_p {
            request.top_p = Some(top_p);
        }
        if let Some(max_tokens) = self.options.max_completion_tokens {
            request.max_completion_tokens = Some(max_tokens);
        }
        if let Some(freq_pen) = self.options.frequency_penalty {
            request.frequency_penalty = Some(freq_pen);
        }
        if let Some(pres_pen) = self.options.presence_penalty {
            request.presence_penalty = Some(pres_pen);
        }
        if !self.options.stop_sequences.is_empty() {
            request.stop = Some(Stop::StringArray(self.options.stop_sequences.clone()));
        }
        if let Some(tools) = tools {
            request.tools = Some(tools);
        }

        request
    }

    pub fn create_client_config(&self) -> impl async_openai::config::Config {
        NysaOpenAiConfig {
            base_url: self.base_url.clone(),
            api_key: SecretString::from(self.api_key.clone()),
        }
    }
}

#[derive(Clone)]
pub struct EmbeddingConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub dimensions: Option<u32>,
    pub encoding_format: Option<EncodingFormat>,
}

impl EmbeddingConfig {
    pub fn to_openai_request(
        &self,
        input: async_openai::types::EmbeddingInput,
    ) -> async_openai::types::CreateEmbeddingRequest {
        async_openai::types::CreateEmbeddingRequest {
            model: self.model.clone(),
            input,
            dimensions: self.dimensions,
            encoding_format: self.encoding_format.clone(),
            user: None,
        }
    }

    pub fn create_client_config(&self) -> impl async_openai::config::Config {
        NysaOpenAiConfig {
            base_url: self.base_url.clone(),
            api_key: SecretString::from(self.api_key.clone()),
        }
    }
}

#[derive(Clone)]
struct NysaOpenAiConfig {
    base_url: String,
    api_key: SecretString,
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

pub struct ChatConfigBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    options: ChatOptions,
}

impl ChatConfigBuilder {
    pub fn new() -> Self {
        Self {
            base_url: None,
            api_key: None,
            model: None,
            options: ChatOptions::default(),
        }
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        self.options.temperature = Some(temp);
        self
    }

    pub fn top_p(mut self, top_p: f32) -> Self {
        self.options.top_p = Some(top_p);
        self
    }

    pub fn max_completion_tokens(mut self, max: u32) -> Self {
        self.options.max_completion_tokens = Some(max);
        self
    }

    pub fn frequency_penalty(mut self, penalty: f32) -> Self {
        self.options.frequency_penalty = Some(penalty);
        self
    }

    pub fn presence_penalty(mut self, penalty: f32) -> Self {
        self.options.presence_penalty = Some(penalty);
        self
    }

    pub fn stop_sequence(mut self, seq: impl Into<String>) -> Self {
        self.options.stop_sequences.push(seq.into());
        self
    }

    pub fn build(self) -> anyhow::Result<ChatConfig> {
        Ok(ChatConfig {
            base_url: self
                .base_url
                .ok_or_else(|| anyhow!("base_url is required"))?,
            api_key: self.api_key.ok_or_else(|| anyhow!("api_key is required"))?,
            model: self.model.ok_or_else(|| anyhow!("model is required"))?,
            options: self.options,
        })
    }
}

impl Default for ChatConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EmbeddingConfigBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    dimensions: Option<u32>,
    encoding_format: Option<EncodingFormat>,
}

impl EmbeddingConfigBuilder {
    pub fn new() -> Self {
        Self {
            base_url: None,
            api_key: None,
            model: None,
            dimensions: None,
            encoding_format: None,
        }
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
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
        Ok(EmbeddingConfig {
            base_url: self
                .base_url
                .ok_or_else(|| anyhow!("base_url is required"))?,
            api_key: self.api_key.ok_or_else(|| anyhow!("api_key is required"))?,
            model: self.model.ok_or_else(|| anyhow!("model is required"))?,
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

pub struct AiConfigBuilder {
    chat: Option<ChatConfig>,
    embedding: Option<EmbeddingConfig>,
}

impl AiConfigBuilder {
    pub fn new() -> Self {
        Self {
            chat: None,
            embedding: None,
        }
    }

    pub fn chat(mut self, config: ChatConfig) -> Self {
        self.chat = Some(config);
        self
    }

    pub fn embedding(mut self, config: EmbeddingConfig) -> Self {
        self.embedding = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<AiConfig> {
        Ok(AiConfig {
            chat: self
                .chat
                .ok_or_else(|| anyhow!("chat config is required"))?,
            embedding: self
                .embedding
                .ok_or_else(|| anyhow!("embedding config is required"))?,
        })
    }
}

impl Default for AiConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
