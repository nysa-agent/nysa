use async_openai::{
    types::*,
    Client as OpenAIClient,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;

use crate::config::ai::{AiConfig, ChatOptions, NysaOpenAiConfig, ChatProvider, SummarizationProvider};
use crate::llm::types::*;
use crate::llm::tokenizer::estimate_messages_tokens;

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamDelta, LlmError>> + Send>>;

#[derive(Clone)]
pub struct LlmClient {
    client: OpenAIClient<NysaOpenAiConfig>,
    chat_options: ChatOptions,
}

impl LlmClient {
    pub fn from_config(config: &AiConfig) -> Self {
        let client_config = config.provider.to_openai_config();
        let client = OpenAIClient::with_config(client_config);

        Self {
            client,
            chat_options: config.chat.options.clone(),
        }
    }

    pub fn from_provider(provider: &crate::config::ai::Provider) -> Self {
        let client_config = provider.to_openai_config();
        let client = OpenAIClient::with_config(client_config);

        Self {
            client,
            chat_options: ChatOptions::default(),
        }
    }

    pub fn with_options(mut self, options: ChatOptions) -> Self {
        self.chat_options = options;
        self
    }

    pub async fn complete(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<LlmResponse, LlmError> {
        self.complete_for_model("default", messages, tools).await
    }

    pub async fn stream(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamDelta, LlmError>> + Send>>, LlmError> {
        self.stream_for_model("default", messages, tools).await
    }

    pub async fn complete_for_model(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<LlmResponse, LlmError> {
        let request = self.build_request(model, messages, tools)?;

        let response = self
            .client
            .chat()
            .create(request)
            .await?;

        self.parse_response(response)
    }

    pub async fn stream_for_model(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamDelta, LlmError>> + Send>>, LlmError> {
        let request = self.build_request(model, messages, tools)?;
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
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<CreateChatCompletionRequest, LlmError> {
        let mut request = CreateChatCompletionRequest {
            model: model.to_string(),
            messages,
            ..Default::default()
        };

        if let Some(temp) = self.chat_options.temperature {
            request.temperature = Some(temp);
        }
        if let Some(top_p) = self.chat_options.top_p {
            request.top_p = Some(top_p);
        }
        if let Some(max_tokens) = self.chat_options.max_completion_tokens {
            request.max_completion_tokens = Some(max_tokens);
        }
        if let Some(freq_pen) = self.chat_options.frequency_penalty {
            request.frequency_penalty = Some(freq_pen);
        }
        if let Some(pres_pen) = self.chat_options.presence_penalty {
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
}

#[async_trait]
impl ChatProvider for LlmClient {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<LlmResponse, LlmError> {
        self.complete_for_model(model, messages, None).await
    }
}

#[async_trait]
impl SummarizationProvider for LlmClient {
    async fn summarize(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<String, LlmError> {
        let response = self.complete_for_model(model, messages, None).await?;
        Ok(response.content.unwrap_or_default())
    }
}

pub fn create_user_message(content: impl Into<String>) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
        content: ChatCompletionRequestUserMessageContent::Text(content.into()),
        ..Default::default()
    })
}

pub fn create_system_message(content: impl Into<String>) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
        content: ChatCompletionRequestSystemMessageContent::Text(content.into()),
        ..Default::default()
    })
}

pub fn create_assistant_message(
    content: Option<impl Into<String>>,
    tool_calls: Option<Vec<ChatCompletionMessageToolCall>>,
) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
        content: content.map(|c| ChatCompletionRequestAssistantMessageContent::Text(c.into())),
        tool_calls,
        ..Default::default()
    })
}

pub fn create_tool_message(
    tool_call_id: impl Into<String>,
    content: impl Into<String>,
) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
        tool_call_id: tool_call_id.into(),
        content: ChatCompletionRequestToolMessageContent::Text(content.into()),
    })
}
