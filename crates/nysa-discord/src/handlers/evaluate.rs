use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handler for "Evaluate All" mode
/// In this mode, every message is evaluated by the LLM to determine
/// if a response is warranted
pub struct EvaluateAllHandler {
    recent_messages: Arc<RwLock<HashMap<u64, VecDeque<RecentMessage>>>>,
    context_window_size: usize,
}

#[derive(Debug, Clone)]
struct RecentMessage {
    content: String,
    author_id: u64,
    timestamp: chrono::DateTime<Utc>,
    responded: bool,
}

impl Clone for EvaluateAllHandler {
    fn clone(&self) -> Self {
        Self {
            recent_messages: Arc::clone(&self.recent_messages),
            context_window_size: self.context_window_size,
        }
    }
}

/// Response criteria for evaluation
#[derive(Debug, Clone)]
pub struct ResponseCriteria {
    /// Direct mention or address
    pub is_addressed: bool,
    /// Contains question marks
    pub has_question: bool,
    /// Relevant to recent conversation
    pub is_contextual: bool,
    /// Contains keywords indicating help needed
    pub needs_assistance: bool,
    /// Conversation has been one-sided (user talking a lot)
    pub user_engaged: bool,
}

impl EvaluateAllHandler {
    pub fn new() -> Self {
        let recent_messages: Arc<RwLock<HashMap<u64, VecDeque<RecentMessage>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let messages_clone = Arc::clone(&recent_messages);

        // Cleanup old messages periodically
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;

                let cutoff = Utc::now() - chrono::Duration::hours(1);
                let mut messages = messages_clone.write().await;

                for (_, msgs) in messages.iter_mut() {
                    msgs.retain(|m| m.timestamp > cutoff);
                }

                messages.retain(|_, msgs| !msgs.is_empty());
            }
        });

        Self {
            recent_messages,
            context_window_size: 10,
        }
    }

    /// Record a message for context evaluation
    pub async fn record_message(&self, channel_id: u64, content: String, author_id: u64) {
        let mut messages = self.recent_messages.write().await;
        let entry = messages.entry(channel_id).or_default();

        entry.push_back(RecentMessage {
            content,
            author_id,
            timestamp: Utc::now(),
            responded: false,
        });

        // Keep only recent messages
        if entry.len() > self.context_window_size {
            entry.pop_front();
        }
    }

    /// Mark that we responded to a message in this channel
    pub async fn mark_responded(&self, channel_id: u64) {
        let mut messages = self.recent_messages.write().await;
        if let Some(msgs) = messages.get_mut(&channel_id)
            && let Some(last) = msgs.back_mut()
        {
            last.responded = true;
        }
    }

    /// Evaluate whether we should respond to this message
    /// Returns a score from 0.0 to 1.0, where 1.0 means definitely respond
    pub async fn evaluate(&self, channel_id: u64, content: &str, author_id: u64) -> f64 {
        let messages = self.recent_messages.read().await;
        let channel_history = messages.get(&channel_id);

        let criteria = self.analyze_criteria(content, author_id, channel_history);

        // Calculate response score
        let mut score: f64 = 0.0;

        if criteria.is_addressed {
            score += 0.4;
        }

        if criteria.has_question {
            score += 0.25;
        }

        if criteria.needs_assistance {
            score += 0.2;
        }

        if criteria.is_contextual {
            score += 0.1;
        }

        if criteria.user_engaged {
            score += 0.05;
        }

        // Check if we recently responded to avoid spam
        if let Some(history) = channel_history {
            let recent_responses = history.iter().rev().take(3).filter(|m| m.responded).count();
            if recent_responses >= 2 {
                // We've responded a lot recently, be more conservative
                score *= 0.7;
            }
        }

        score.min(1.0)
    }

    /// Should we respond based on the score?
    pub async fn should_respond(
        &self,
        channel_id: u64,
        content: &str,
        author_id: u64,
        threshold: f64,
    ) -> bool {
        let score = self.evaluate(channel_id, content, author_id).await;
        score >= threshold
    }

    fn analyze_criteria(
        &self,
        content: &str,
        author_id: u64,
        channel_history: Option<&VecDeque<RecentMessage>>,
    ) -> ResponseCriteria {
        let content_lower = content.to_lowercase();

        // Check for direct mention patterns
        let is_addressed = content_lower.contains("nysa")
            || content_lower.contains("@nysa")
            || content_lower.starts_with("hey ")
            || content_lower.starts_with("hi ")
            || content_lower.starts_with("hello ");

        // Check for questions
        let has_question = content.contains('?')
            || content_lower.contains("how")
            || content_lower.contains("what")
            || content_lower.contains("why")
            || content_lower.contains("when")
            || content_lower.contains("where")
            || content_lower.contains("who")
            || content_lower.contains("can you")
            || content_lower.contains("could you");

        // Check for assistance keywords
        let needs_assistance = content_lower.contains("help")
            || content_lower.contains("assist")
            || content_lower.contains("support")
            || content_lower.contains("stuck")
            || content_lower.contains("problem")
            || content_lower.contains("issue")
            || content_lower.contains("error");

        // Analyze context
        let (is_contextual, user_engaged) = if let Some(history) = channel_history {
            // Check if this continues a conversation
            let recent_user_msgs: Vec<_> = history
                .iter()
                .rev()
                .take(5)
                .filter(|m| m.author_id == author_id)
                .collect();

            let user_engaged = recent_user_msgs.len() >= 3;

            // Contextual if we've been talking recently
            let is_contextual = history.iter().rev().take(3).any(|m| m.responded);

            (is_contextual, user_engaged)
        } else {
            (false, false)
        };

        ResponseCriteria {
            is_addressed,
            has_question,
            is_contextual,
            needs_assistance,
            user_engaged,
        }
    }

    /// Get conversation context for the LLM
    pub async fn get_context(&self, channel_id: u64) -> Vec<(String, u64)> {
        let messages = self.recent_messages.read().await;
        messages
            .get(&channel_id)
            .map(|msgs| {
                msgs.iter()
                    .map(|m| (m.content.clone(), m.author_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear context for a channel
    pub async fn clear_context(&self, channel_id: u64) {
        let mut messages = self.recent_messages.write().await;
        messages.remove(&channel_id);
    }
}

impl Default for EvaluateAllHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for evaluation thresholds
#[derive(Debug, Clone)]
pub struct EvaluateConfig {
    /// Minimum score to respond (0.0 - 1.0)
    pub response_threshold: f64,
    /// Maximum messages to keep in context
    pub max_context_messages: usize,
    /// Timeout for considering messages as part of same conversation (minutes)
    pub conversation_timeout_minutes: i64,
}

impl Default for EvaluateConfig {
    fn default() -> Self {
        Self {
            response_threshold: 0.6,
            max_context_messages: 10,
            conversation_timeout_minutes: 30,
        }
    }
}
