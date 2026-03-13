//! Simple token estimation without external tokenizer dependencies
//! Uses character-based and word-based heuristics

use async_openai::types::ChatCompletionRequestMessage;

/// Roughly estimate the number of tokens in a string
/// GPT-4 uses roughly 4 characters per token on average
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let char_estimate = (text.len() as f64 / 4.0).ceil() as usize;
    let words = text.split_whitespace().count();
    let word_estimate = (words as f64 * 1.3).ceil() as usize;

    ((char_estimate + word_estimate) / 2).max(1)
}

/// Estimate tokens for a single message
pub fn estimate_message_tokens(message: &ChatCompletionRequestMessage) -> usize {
    // Base overhead for each message
    let base_overhead = 4;

    let content_len = match message {
        ChatCompletionRequestMessage::System(_) => 0,
        ChatCompletionRequestMessage::User(_) => 0,
        ChatCompletionRequestMessage::Assistant(_) => 0,
        ChatCompletionRequestMessage::Tool(_) => 0,
        ChatCompletionRequestMessage::Function(_) => 0,
        _ => 0,
    };

    base_overhead + content_len
}

/// Estimate total tokens for a conversation
pub fn estimate_messages_tokens(messages: &[ChatCompletionRequestMessage]) -> usize {
    if messages.is_empty() {
        return 0;
    }

    // Base overhead for the entire request
    let base_tokens = 3;

    let message_tokens: usize = messages.iter().map(estimate_message_tokens).sum();

    base_tokens + message_tokens
}

/// Check if a conversation is approaching the context limit
pub fn is_approaching_limit(current_tokens: usize, max_tokens: usize, threshold: f32) -> bool {
    (current_tokens as f32 / max_tokens as f32) >= threshold
}

/// Calculate remaining tokens for response
pub fn calculate_remaining_tokens(used_tokens: usize, max_tokens: usize) -> usize {
    max_tokens.saturating_sub(used_tokens)
}
