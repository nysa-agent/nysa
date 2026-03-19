use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder,
    Set,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::config::ai::{AiConfig, CompactionConfig, SummarizationProvider};
use crate::database::entities::message::{
    ActiveModel as MessageActiveModel, Column, Entity as MessageEntity,
};
use crate::llm::client::{LlmClient, create_system_message, create_user_message};
use crate::llm::types::LlmError;

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("AI error: {0}")]
    Ai(String),
    #[error("Thread not found: {0}")]
    ThreadNotFound(Uuid),
    #[error("No messages to compact")]
    NoMessages,
    #[error("LLM not configured")]
    LlmNotConfigured,
}

impl From<sea_orm::DbErr> for CompactionError {
    fn from(err: sea_orm::DbErr) -> Self {
        CompactionError::Database(err.to_string())
    }
}

impl From<LlmError> for CompactionError {
    fn from(err: LlmError) -> Self {
        CompactionError::Ai(err.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub thread_id: Uuid,
    pub original_message_count: usize,
    pub compacted_message_count: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub message_id: Uuid,
    pub content: String,
    pub created_at: String,
    pub author_name: String,
    pub similarity: f32,
}

#[derive(Clone)]
pub struct CompactionService {
    db: DatabaseConnection,
    llm_client: Option<LlmClient>,
    config: CompactionConfig,
    compaction_model: String,
}

impl CompactionService {
    pub fn new(db: DatabaseConnection, ai_config: &AiConfig) -> Self {
        let llm_client = if ai_config.compaction.enabled {
            Some(LlmClient::from_provider(ai_config.compaction_provider()))
        } else {
            None
        };

        let compaction_model = ai_config.compaction_model_or_default().to_string();

        Self {
            db,
            llm_client,
            config: ai_config.compaction.clone(),
            compaction_model,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.llm_client.is_some()
    }

    pub async fn compact_thread(
        &self,
        thread_id: Uuid,
    ) -> Result<CompactionResult, CompactionError> {
        if !self.is_enabled() {
            tracing::debug!("Compaction disabled or LLM not configured, skipping");
            return Err(CompactionError::Ai("Compaction not available".to_string()));
        }

        tracing::info!("Compacting thread {}", thread_id);

        let messages = MessageEntity::find()
            .filter(Column::ThreadId.eq(thread_id))
            .order_by(Column::CreatedAt, Order::Asc)
            .all(&self.db)
            .await?;

        let original_count = messages.len();
        let preserve_count = self.config.preserve_recent;

        if original_count <= preserve_count {
            tracing::debug!(
                "Thread {} has fewer messages than preserve_recent ({})",
                thread_id,
                preserve_count
            );
            return Err(CompactionError::NoMessages);
        }

        let to_compact = original_count - preserve_count;

        if to_compact == 0 {
            return Err(CompactionError::NoMessages);
        }

        let messages_for_summary: Vec<_> = messages.iter().take(to_compact).cloned().collect();

        let summary = self.generate_summary(&messages_for_summary).await?;

        for msg in messages_for_summary {
            let delete_model: MessageActiveModel = msg.into();
            delete_model.delete(&self.db).await?;
        }

        let summary_message = MessageActiveModel {
            id: Set(Uuid::new_v4()),
            thread_id: Set(thread_id),
            platform_message_id: Set(None),
            author_internal_id: Set(None),
            author_platform_id: Set(None),
            author_name: Set("System".to_string()),
            content: Set(format!("[Compacted History Summary]\n\n{}", summary)),
            role: Set("assistant".to_string()),
            created_at: Set(chrono::Utc::now().naive_utc()),
        };
        summary_message.insert(&self.db).await?;

        tracing::info!(
            "Compacted thread {}: {} messages -> 1 summary + {} preserved",
            thread_id,
            original_count,
            self.config.preserve_recent
        );

        Ok(CompactionResult {
            thread_id,
            original_message_count: original_count,
            compacted_message_count: 1 + self.config.preserve_recent,
            summary,
        })
    }

    async fn generate_summary(
        &self,
        messages: &[crate::database::entities::message::Model],
    ) -> Result<String, CompactionError> {
        let llm_client = self
            .llm_client
            .as_ref()
            .ok_or(CompactionError::LlmNotConfigured)?;

        let conversation_text = messages
            .iter()
            .map(|m| format!("{}: {}", m.author_name, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let summary_prompt = format!(
            "The following is a conversation transcript. Please provide a concise summary that captures the key points, topics discussed, and any important conclusions or decisions made. This summary will be used to preserve context when the conversation becomes too long for the AI's context window.\n\nConversation:\n{}\n\nSummary:",
            conversation_text
        );

        let request_messages = vec![
            create_system_message(
                "You are a helpful assistant that summarizes conversations concisely.",
            ),
            create_user_message(&summary_prompt),
        ];

        let response = llm_client
            .summarize(&self.compaction_model, request_messages)
            .await?;

        Ok(response)
    }

    pub async fn get_thread_context(
        &self,
        thread_id: Uuid,
        _query: &str,
    ) -> Result<Vec<SearchResult>, CompactionError> {
        tracing::info!("Getting context for thread {}", thread_id);

        Ok(Vec::new())
    }

    pub async fn trigger_compaction(
        &self,
        thread_id: Uuid,
    ) -> Result<CompactionResult, CompactionError> {
        self.compact_thread(thread_id).await
    }
}

pub struct CompactionManager {
    db: DatabaseConnection,
}

impl CompactionManager {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub fn service(&self, ai_config: &AiConfig) -> CompactionService {
        CompactionService::new(self.db.clone(), ai_config)
    }

    pub async fn cleanup_old_threads(
        &self,
        _idle_threshold: chrono::Duration,
    ) -> Result<usize, CompactionError> {
        Ok(0)
    }
}
