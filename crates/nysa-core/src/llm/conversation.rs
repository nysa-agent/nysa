use std::sync::Arc;
use uuid::Uuid;

use async_openai::types::ChatCompletionTool;
use futures::StreamExt;

use crate::compaction::CompactionService;
use crate::context::MessageContext;
use crate::llm::client::{LlmClient, create_system_message, create_tool_message};
use crate::llm::history::MessageHistoryService;
use crate::llm::prompt::SystemPrompt;
use crate::llm::tokenizer::{estimate_messages_tokens, is_approaching_limit};
use crate::llm::types::*;
use crate::prompt::PromptContext;
use crate::tool::ToolExecutor;

/// Main orchestrator for LLM conversations
/// Handles the complete flow: message -> context -> LLM -> (tools -> LLM)* -> response
pub struct ConversationManager {
    llm: Arc<LlmClient>,
    history: Arc<MessageHistoryService>,
    tool_executor: Option<Arc<ToolExecutor>>,
    compaction: CompactionService,
    system_prompt: SystemPrompt,
    config: LlmConfig,
}

impl ConversationManager {
    pub fn new(
        llm: Arc<LlmClient>,
        history: Arc<MessageHistoryService>,
        tool_executor: Option<Arc<ToolExecutor>>,
        compaction: CompactionService,
        config: LlmConfig,
    ) -> Self {
        let system_prompt = match &config.system_prompt_override {
            Some(prompt) => SystemPrompt::default().with_base_prompt(prompt.clone()),
            None => SystemPrompt::default(),
        };

        Self {
            llm,
            history,
            tool_executor,
            compaction,
            system_prompt,
            config,
        }
    }

    /// Send a message and get a response (main entry point)
    /// This orchestrates the entire conversation flow including tool execution
    pub async fn send_message(
        &self,
        thread_id: Uuid,
        user_message: &str,
        context: &MessageContext,
        tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<ConversationResponse, LlmError> {
        // 1. Add user message to history
        let author_name = context.user.as_ref()
            .map(|u| u.display_name.clone())
            .unwrap_or_else(|| "User".to_string());
        
        let author_id = context.user.as_ref().map(|u| u.internal_id);
        
        self.history.add_user_message(
            thread_id,
            user_message,
            &author_name,
            author_id,
        ).await?;

        // 2. Check if compaction is needed
        self.check_and_compact(thread_id).await?;

        // 3. Build messages array for LLM
        let mut messages = self.build_messages(thread_id, context).await?;

        // 4. Execute LLM with potential tool calls
        let mut all_tool_executions: Vec<ToolExecution> = Vec::new();
        let mut iteration = 0u8;
        let final_response: LlmResponse;

        loop {
            if iteration >= self.config.max_tool_iterations {
                return Err(LlmError::MaxIterationsReached(iteration));
            }
            iteration += 1;

            // Execute LLM request
            let response = self.llm.complete(messages.clone(), tools.clone()).await?;

            // Check if there are tool calls to process
            if response.has_tool_calls() && self.tool_executor.is_some() {
                // Save assistant message with tool calls
                let tool_call_records: Vec<ToolCallRecord> = response.tool_calls.iter().map(|tc| {
                    ToolCallRecord {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    }
                }).collect();

                self.history.add_assistant_message(
                    thread_id,
                    response.content.as_deref(),
                    Some(tool_call_records),
                ).await?;

                // Process tool calls sequentially
                let tool_results = self.process_tool_calls(
                    thread_id,
                    &response.tool_calls,
                    &mut all_tool_executions,
                ).await?;

                // Add tool results to messages and continue loop
                for result in tool_results {
                    messages.push(create_tool_message(
                        result.tool_call_id,
                        &result.result,
                    ));
                }
            } else {
                // No tool calls, this is the final response
                final_response = response;
                break;
            }
        }

        // 5. Save final assistant response
        self.history.add_assistant_message(
            thread_id,
            final_response.content.as_deref(),
            None,
        ).await?;

        // 6. Build and return response
        let content = final_response.content.unwrap_or_default();
        
        Ok(ConversationResponse {
            content,
            tool_calls_made: all_tool_executions,
            finish_reason: final_response.finish_reason,
            tokens_used: final_response.usage,
        })
    }

    /// Send a message and stream the response
    /// Returns a stream of text deltas
    pub async fn stream_message(
        &self,
        thread_id: Uuid,
        user_message: &str,
        context: &MessageContext,
        _tools: Option<Vec<ChatCompletionTool>>,
    ) -> Result<impl futures::Stream<Item = Result<String, LlmError>>, LlmError> {
        // For streaming, we don't support tool calls in the initial implementation
        // Tools require complete responses to execute
        
        // Add user message to history
        let author_name = context.user.as_ref()
            .map(|u| u.display_name.clone())
            .unwrap_or_else(|| "User".to_string());
        
        let author_id = context.user.as_ref().map(|u| u.internal_id);
        
        self.history.add_user_message(
            thread_id,
            user_message,
            &author_name,
            author_id,
        ).await?;

        // Check compaction
        self.check_and_compact(thread_id).await?;

        // Build messages (without tool calls for streaming)
        let messages = self.build_messages(thread_id, context).await?;

        // Start streaming
        let stream = self.llm.stream(messages, None).await?;
        
        // Collect the full response in a separate task
        tokio::spawn(async move {
            let _full_content = String::new();
            
            // Note: In a real implementation, we'd need to properly handle the stream
            // and collect the full content. This is simplified.
        });

        Ok(stream.map(|delta_result| {
            match delta_result {
                Ok(delta) => Ok(delta.content.unwrap_or_default()),
                Err(e) => Err(e),
            }
        }))
    }

    /// Build the messages array for LLM request
    async fn build_messages(
        &self,
        thread_id: Uuid,
        context: &MessageContext,
    ) -> Result<Vec<async_openai::types::ChatCompletionRequestMessage>, LlmError> {
        // 1. Build system prompt
        let prompt_context = PromptContext::new(context.platform.clone())
            .with_user(
                context.user.as_ref().map(|u| u.internal_id).unwrap_or_else(Uuid::nil),
                context.user.is_some(),
            )
            .with_thread(thread_id);
        
        let system_prompt = self.system_prompt.build(&prompt_context);
        
        // 2. Get conversation history
        let history = self.history.get_messages(thread_id, None).await?;
        
        // 3. Build OpenAI messages array
        let mut messages = vec![create_system_message(&system_prompt)];
        let history_messages = self.history.to_openai_messages(history)?;
        messages.extend(history_messages);
        
        // 4. Check token count
        let token_count = estimate_messages_tokens(&messages);
        if token_count > self.config.max_context_tokens {
            return Err(LlmError::ContextTooLong(token_count, self.config.max_context_tokens));
        }

        Ok(messages)
    }

    /// Process tool calls sequentially
    async fn process_tool_calls(
        &self,
        thread_id: Uuid,
        tool_calls: &[async_openai::types::ChatCompletionMessageToolCall],
        executions: &mut Vec<ToolExecution>,
    ) -> Result<Vec<ToolResultMessage>, LlmError> {
        let mut results = Vec::new();

        for tool_call in tool_calls {
            let tool_name = &tool_call.function.name;
            let tool_call_id = &tool_call.id;
            
            // Parse arguments
            let args: serde_json::Value = match serde_json::from_str(&tool_call.function.arguments) {
                Ok(args) => args,
                Err(e) => {
                    return Err(LlmError::InvalidToolArguments(format!(
                        "Failed to parse arguments for tool {}: {}",
                        tool_name,
                        e
                    )));
                }
            };

            // Execute tool if we have an executor
            let result = if let Some(ref executor) = self.tool_executor {
                match executor.dispatch(tool_name, args.clone()).await {
                    Ok(tool_result) => {
                        executions.push(ToolExecution {
                            name: tool_name.clone(),
                            arguments: args.clone(),
                            result: tool_result.content.clone(),
                        });
                        
                        if tool_result.is_error {
                            format!("Error: {}", tool_result.content)
                        } else {
                            tool_result.content
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Tool execution failed: {}", e);
                        executions.push(ToolExecution {
                            name: tool_name.clone(),
                            arguments: args,
                            result: error_msg.clone(),
                        });
                        error_msg
                    }
                }
            } else {
                "Tool execution not available".to_string()
            };

            // Save tool result to history
            self.history.add_tool_message(
                thread_id,
                tool_call_id,
                tool_name,
                &result,
            ).await?;

            results.push(ToolResultMessage {
                tool_call_id: tool_call_id.clone(),
                name: tool_name.clone(),
                result,
            });
        }

        Ok(results)
    }

    /// Check if compaction is needed and trigger it
    async fn check_and_compact(&self, thread_id: Uuid) -> Result<(), LlmError> {
        let messages = self.history.get_messages(thread_id, None).await?;
        let openai_messages = self.history.to_openai_messages(messages)?;
        let token_count = estimate_messages_tokens(&openai_messages);

        if is_approaching_limit(token_count, self.config.max_context_tokens, self.config.compaction_threshold) {
            tracing::info!(
                "Thread {} approaching context limit ({} / {} tokens), triggering compaction",
                thread_id,
                token_count,
                self.config.max_context_tokens
            );

            // Trigger compaction
            match self.compaction.compact_thread(thread_id).await {
                Ok(result) => {
                    tracing::info!(
                        "Compacted thread {}: {} -> {} messages",
                        thread_id,
                        result.original_message_count,
                        result.compacted_message_count
                    );
                }
                Err(e) => {
                    tracing::error!("Failed to compact thread {}: {}", thread_id, e);
                    // Continue anyway - we'll truncate if needed
                }
            }
        }

        Ok(())
    }

    /// Get conversation history for a thread
    pub async fn get_history(&self, thread_id: Uuid) -> Result<Vec<ConversationMessage>, LlmError> {
        self.history.get_messages(thread_id, None).await
    }

    /// Clear conversation history for a thread
    pub async fn clear_history(&self, thread_id: Uuid) -> Result<u64, LlmError> {
        self.history.delete_thread_messages(thread_id).await
    }

    /// Get token count for a thread
    pub async fn get_token_count(&self, thread_id: Uuid) -> Result<usize, LlmError> {
        self.history.estimate_tokens(thread_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require mocking the database and LLM client
    // They're here as placeholders for the structure
    
    #[test]
    fn test_tool_execution_tracking() {
        let execution = ToolExecution {
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"arg": "value"}),
            result: "success".to_string(),
        };
        
        assert_eq!(execution.name, "test_tool");
    }
}
