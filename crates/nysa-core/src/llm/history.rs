use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestMessage, ChatCompletionToolType,
};
use async_openai::types::FunctionCall;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, Order,
    QueryOrder, PaginatorTrait,
};
use uuid::Uuid;

use crate::database::entities::message::{ActiveModel as MessageActiveModel, Column, Entity as MessageEntity};
use crate::llm::types::*;
use crate::llm::tokenizer::estimate_messages_tokens;

/// Service for managing message history in conversations
pub struct MessageHistoryService {
    db: DatabaseConnection,
}

impl MessageHistoryService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Get messages for a thread, ordered by time
    pub async fn get_messages(
        &self,
        thread_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ConversationMessage>, LlmError> {
        let query = MessageEntity::find()
            .filter(Column::ThreadId.eq(thread_id))
            .order_by(Column::CreatedAt, Order::Asc);

        if let Some(limit) = limit {
            // Note: SeaORM doesn't have a direct limit method that takes usize easily
            // So we'll fetch all and truncate
            let messages: Vec<_> = query.all(&self.db).await?;
            let messages: Vec<_> = messages.into_iter()
                .rev()
                .take(limit)
                .rev()
                .collect();
            
            return Ok(messages.into_iter().map(Self::db_to_conversation_message).collect());
        }

        let messages = query.all(&self.db).await?;
        Ok(messages.into_iter().map(Self::db_to_conversation_message).collect())
    }

    /// Get the most recent N messages for a thread
    pub async fn get_recent_messages(
        &self,
        thread_id: Uuid,
        count: usize,
    ) -> Result<Vec<ConversationMessage>, LlmError> {
        let messages = MessageEntity::find()
            .filter(Column::ThreadId.eq(thread_id))
            .order_by(Column::CreatedAt, Order::Desc)
            .all(&self.db)
            .await?;

        let messages: Vec<_> = messages.into_iter()
            .take(count)
            .rev()
            .map(Self::db_to_conversation_message)
            .collect();

        Ok(messages)
    }

    /// Add a user message to the history
    pub async fn add_user_message(
        &self,
        thread_id: Uuid,
        content: &str,
        author_name: &str,
        author_id: Option<Uuid>,
    ) -> Result<Uuid, LlmError> {
        let message_id = Uuid::new_v4();
        
        let message = MessageActiveModel {
            id: Set(message_id),
            thread_id: Set(thread_id),
            platform_message_id: Set(None),
            author_internal_id: Set(author_id),
            author_platform_id: Set(None),
            author_name: Set(author_name.to_string()),
            content: Set(content.to_string()),
            role: Set("user".to_string()),
            created_at: Set(chrono::Utc::now().naive_utc()),
        };

        message.insert(&self.db).await?;
        Ok(message_id)
    }

    /// Add an assistant message (potentially with tool calls)
    pub async fn add_assistant_message(
        &self,
        thread_id: Uuid,
        content: Option<&str>,
        tool_calls: Option<Vec<ToolCallRecord>>,
    ) -> Result<Uuid, LlmError> {
        let message_id = Uuid::new_v4();
        
        // Store tool calls as JSON in content if present
        let final_content = if let Some(ref calls) = tool_calls {
            if let Some(content) = content {
                format!("{}\n\n[Tool Calls: {}]", content, serde_json::to_string(calls).unwrap_or_default())
            } else {
                format!("[Tool Calls: {}]", serde_json::to_string(calls).unwrap_or_default())
            }
        } else {
            content.unwrap_or("").to_string()
        };

        let message = MessageActiveModel {
            id: Set(message_id),
            thread_id: Set(thread_id),
            platform_message_id: Set(None),
            author_internal_id: Set(None),
            author_platform_id: Set(None),
            author_name: Set("Nysa".to_string()),
            content: Set(final_content),
            role: Set("assistant".to_string()),
            created_at: Set(chrono::Utc::now().naive_utc()),
        };

        message.insert(&self.db).await?;
        Ok(message_id)
    }

    /// Add a tool result message
    pub async fn add_tool_message(
        &self,
        thread_id: Uuid,
        tool_call_id: &str,
        tool_name: &str,
        result: &str,
    ) -> Result<Uuid, LlmError> {
        let message_id = Uuid::new_v4();
        
        // Format: include tool_call_id for proper OpenAI API compatibility
        let content = format!("[Tool {} (ID: {}) Result]: {}", tool_name, tool_call_id, result);

        let message = MessageActiveModel {
            id: Set(message_id),
            thread_id: Set(thread_id),
            platform_message_id: Set(None),
            author_internal_id: Set(None),
            author_platform_id: Set(None),
            author_name: Set(format!("tool:{}", tool_name)),
            content: Set(content),
            role: Set("tool".to_string()),
            created_at: Set(chrono::Utc::now().naive_utc()),
        };

        message.insert(&self.db).await?;
        Ok(message_id)
    }

    /// Delete all messages for a thread (e.g., after compaction)
    pub async fn delete_thread_messages(&self, thread_id: Uuid) -> Result<u64, LlmError> {
        let result = MessageEntity::delete_many()
            .filter(Column::ThreadId.eq(thread_id))
            .exec(&self.db)
            .await?;
        
        Ok(result.rows_affected)
    }

    /// Get message count for a thread
    pub async fn get_message_count(&self, thread_id: Uuid) -> Result<u64, LlmError> {
        let count = MessageEntity::find()
            .filter(Column::ThreadId.eq(thread_id))
            .count(&self.db)
            .await?;
        
        Ok(count)
    }

    /// Estimate total tokens for a thread
    pub async fn estimate_tokens(&self, thread_id: Uuid) -> Result<usize, LlmError> {
        let messages = self.get_messages(thread_id, None).await?;
        let openai_messages = self.to_openai_messages(messages)?;
        Ok(estimate_messages_tokens(&openai_messages))
    }

    /// Convert stored messages to OpenAI format for LLM requests
    pub fn to_openai_messages(
        &self,
        messages: Vec<ConversationMessage>,
    ) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
        use crate::llm::client::{create_user_message, create_system_message, create_assistant_message, create_tool_message};

        messages.into_iter().map(|msg| {
            match msg.role {
                MessageRole::System => Ok(create_system_message(&msg.content)),
                MessageRole::User => Ok(create_user_message(&msg.content)),
                MessageRole::Assistant => {
                    // Check if this has embedded tool calls
                    let (content, tool_calls) = if let Some(ref calls) = msg.tool_calls {
                        let openai_calls: Vec<_> = calls.iter().map(|call| {
                            ChatCompletionMessageToolCall {
                                id: call.id.clone(),
                                r#type: ChatCompletionToolType::Function,
                                function: FunctionCall {
                                    name: call.name.clone(),
                                    arguments: call.arguments.clone(),
                                },
                            }
                        }).collect();
                        (Some(msg.content.clone()), Some(openai_calls))
                    } else {
                        (Some(msg.content), None)
                    };
                    
                    Ok(create_assistant_message(content, tool_calls))
                }
                MessageRole::Tool => {
                    // Extract tool call ID from the content if possible
                    let tool_call_id = msg.tool_call_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    Ok(create_tool_message(tool_call_id, &msg.content))
                }
            }
        }).collect()
    }

    /// Convert database model to conversation message
    fn db_to_conversation_message(db_msg: crate::database::entities::message::Model) -> ConversationMessage {
        let role = match db_msg.role.as_str() {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };

        // Try to parse tool calls from content
        let tool_calls = if db_msg.role == "assistant" && db_msg.content.contains("[Tool Calls:") {
            // Extract JSON from [Tool Calls: ...]
            if let Some(start) = db_msg.content.find("[Tool Calls: ") {
                if let Some(end) = db_msg.content[start..].find("]") {
                    let json_str = &db_msg.content[start + 13..start + end];
                    serde_json::from_str::<Vec<ToolCallRecord>>(json_str).ok()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Clean content if it has tool call formatting
        let content = if tool_calls.is_some() {
            db_msg.content.split("\n\n[Tool Calls:").next().unwrap_or(&db_msg.content).to_string()
        } else {
            db_msg.content
        };

        ConversationMessage {
            id: db_msg.id,
            role,
            content,
            author_name: Some(db_msg.author_name),
            tool_calls,
            tool_call_id: None, // We don't store this separately, it's in content
            created_at: chrono::DateTime::from_naive_utc_and_offset(db_msg.created_at, chrono::Utc),
        }
    }

    /// Add a system message to start a conversation
    pub async fn add_system_message(
        &self,
        thread_id: Uuid,
        content: &str,
    ) -> Result<Uuid, LlmError> {
        let message_id = Uuid::new_v4();
        
        let message = MessageActiveModel {
            id: Set(message_id),
            thread_id: Set(thread_id),
            platform_message_id: Set(None),
            author_internal_id: Set(None),
            author_platform_id: Set(None),
            author_name: Set("System".to_string()),
            content: Set(content.to_string()),
            role: Set("system".to_string()),
            created_at: Set(chrono::Utc::now().naive_utc()),
        };

        message.insert(&self.db).await?;
        Ok(message_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_parsing() {
        assert!(matches!(
            MessageHistoryService::db_to_conversation_message(crate::database::entities::message::Model {
                id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                platform_message_id: None,
                author_internal_id: None,
                author_platform_id: None,
                author_name: "Test".to_string(),
                content: "Hello".to_string(),
                role: "user".to_string(),
                created_at: chrono::Utc::now().naive_utc(),
            }).role,
            MessageRole::User
        ));
    }
}
