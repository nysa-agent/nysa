use async_openai::{
    types::*,
    Client as OpenAIClient,
};
use futures::{Stream, StreamExt};
use std::pin::Pin;

use crate::config::ChatConfig;
use crate::llm::types::*;
use crate::llm::tokenizer::estimate_messages_tokens;

pub struct LlmClient {
    client: OpenAIClient<NysaOpenAiConfig>,
    config: ChatConfig,
}

impl LlmClient {
    pub fn new(config: &ChatConfig) -> Self {
        let client_config = NysaOpenAiConfig {
            base_url: config.base_url.clone(),
            api_key: config.api_key.clone(),
        };

        let client = OpenAIClient::with_config(client_config);

        Self {
            client,
            config: config.clone(),
        }
    }

    pub async fn complete(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<LlmResponse, LlmError> {
        let request = self.build_request(messages, tools)?;

        let response = self
            .client
            .chat()
            .create(request)
            .await?;

        self.parse_response(response)
    }

    pub async fn stream(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamDelta, LlmError>> + Send>>, LlmError> {
        let request = self.build_request(messages, tools)?;
        let mut stream_request = request;
        stream_request.stream = Some(true);

        let stream = self
            .client
            .chat()
            .create_stream(stream_request)
            .await?;

        let mapped_stream = stream.map(|result| {
            match result {
                Ok(chunk) => {
                    let delta = chunk.choices.get(0).map(|choice| {
                        StreamDelta {
                            content: choice.delta.content.clone(),
                            tool_calls: choice.delta.tool_calls.as_ref().map(|calls| {
                                calls.iter().map(|call| ToolCallDelta {
                                    index: call.index as usize,
                                    id: call.id.clone(),
                                    name: call.function.as_ref().and_then(|f| f.name.clone()),
                                    arguments: call.function.as_ref().and_then(|f| f.arguments.clone()),
                                }).collect()
                            }),
                            finish_reason: choice.finish_reason.clone(),
                        }
                    });
                    
                    match delta {
                        Some(d) => Ok(d),
                        None => Err(LlmError::NoResponse),
                    }
                }
                Err(e) => Err(LlmError::StreamingError(e.to_string())),
            }
        });

        Ok(Box::pin(mapped_stream))
    }

    fn build_request(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<CreateChatCompletionRequest, LlmError> {
        let mut request = CreateChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            ..Default::default()
        };

        if let Some(temp) = self.config.options.temperature {
            request.temperature = Some(temp);
        }
        if let Some(top_p) = self.config.options.top_p {
            request.top_p = Some(top_p);
        }
        if let Some(max_tokens) = self.config.options.max_completion_tokens {
            request.max_completion_tokens = Some(max_tokens);
        }
        if let Some(freq_pen) = self.config.options.frequency_penalty {
            request.frequency_penalty = Some(freq_pen);
        }
        if let Some(pres_pen) = self.config.options.presence_penalty {
            request.presence_penalty = Some(pres_pen);
        }

        if let Some(tools) = tools {
            request.tools = Some(tools);
        }

        Ok(request)
    }

    fn parse_response(
        &self,
        response: CreateChatCompletionResponse,
    ) -> Result<LlmResponse, LlmError> {
        let choice = response.choices
            .into_iter()
            .next()
            .ok_or(LlmError::NoResponse)?;

        let content = choice.message.content;
        let tool_calls = choice.message.tool_calls.unwrap_or_default();
        let finish_reason = choice.finish_reason.unwrap_or(FinishReason::Stop);

        Ok(LlmResponse {
            content,
            tool_calls,
            finish_reason,
            usage: response.usage,
        })
    }

    pub fn estimate_tokens(&self, messages: &[ChatCompletionRequestMessage]) -> usize {
        estimate_messages_tokens(messages)
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }
}

#[derive(Clone)]
pub struct NysaOpenAiConfig {
    base_url: String,
    api_key: String,
}

impl async_openai::config::Config for NysaOpenAiConfig {
    fn headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, AUTHORIZATION, HeaderValue};
        
        let mut headers = HeaderMap::new();
        
        let auth_value = format!("Bearer {}", self.api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).unwrap_or_else(|_| HeaderValue::from_static("")),
        );

        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://nysa.phrolova.moe/"),
        );
        headers.insert(
            "X-Title",
            HeaderValue::from_static("Nysa"),
        );

        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn api_base(&self) -> &str {
        &self.base_url
    }

    fn api_key(&self) -> &secrecy::SecretString {
        use secrecy::SecretString;
        use std::sync::OnceLock;
        static SECRET: OnceLock<SecretString> = OnceLock::new();
        SECRET.get_or_init(|| SecretString::from(self.api_key.clone()))
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}

pub fn create_user_message(content: impl Into<String>) -> ChatCompletionRequestMessage {
    use async_openai::types::ChatCompletionRequestUserMessage;
    
    ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
        content: ChatCompletionRequestUserMessageContent::Text(content.into()),
        ..Default::default()
    })
}

pub fn create_system_message(content: impl Into<String>) -> ChatCompletionRequestMessage {
    use async_openai::types::ChatCompletionRequestSystemMessage;
    use async_openai::types::ChatCompletionRequestSystemMessageContent;
    
    ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
        content: ChatCompletionRequestSystemMessageContent::Text(content.into()),
        ..Default::default()
    })
}

pub fn create_assistant_message(
    content: Option<impl Into<String>>,
    tool_calls: Option<Vec<ChatCompletionMessageToolCall>>,
) -> ChatCompletionRequestMessage {
    use async_openai::types::ChatCompletionRequestAssistantMessage;
    use async_openai::types::ChatCompletionRequestAssistantMessageContent;
    
    let assistant_message = ChatCompletionRequestAssistantMessage {
        content: content.map(|c| ChatCompletionRequestAssistantMessageContent::Text(c.into())),
        tool_calls,
        ..Default::default()
    };
    
    ChatCompletionRequestMessage::Assistant(assistant_message)
}

pub fn create_tool_message(
    tool_call_id: impl Into<String>,
    content: impl Into<String>,
) -> ChatCompletionRequestMessage {
    use async_openai::types::ChatCompletionRequestToolMessage;
    use async_openai::types::ChatCompletionRequestToolMessageContent;
    
    ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
        tool_call_id: tool_call_id.into(),
        content: ChatCompletionRequestToolMessageContent::Text(content.into()),
    })
}
