use crate::context::Platform;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptCondition {
    pub platform: Option<Platform>,
    pub user_authenticated: bool,
    pub thread_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSection {
    pub name: &'static str,
    pub priority: u8,
    pub content: String,
    pub condition: Option<PromptCondition>,
}

impl PromptSection {
    pub fn new(name: &'static str, priority: u8, content: impl Into<String>) -> Self {
        Self {
            name,
            priority,
            content: content.into(),
            condition: None,
        }
    }

    pub fn with_condition(mut self, condition: PromptCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    pub fn should_include(&self, ctx: &PromptContext) -> bool {
        if let Some(ref cond) = self.condition {
            if let Some(platform) = &cond.platform
                && ctx.platform != *platform
            {
                return false;
            }
            if ctx.user_authenticated != cond.user_authenticated {
                return false;
            }
            if let Some(thread_id) = cond.thread_id
                && ctx.thread_id != Some(thread_id)
            {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContext {
    pub platform: Platform,
    pub user_id: Option<uuid::Uuid>,
    pub user_authenticated: bool,
    pub thread_id: Option<uuid::Uuid>,
    pub channel_id: Option<String>,
    pub guild_id: Option<String>,
    pub custom: serde_json::Value,
}

impl PromptContext {
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            user_id: None,
            user_authenticated: false,
            thread_id: None,
            channel_id: None,
            guild_id: None,
            custom: serde_json::Value::Null,
        }
    }

    pub fn with_user(mut self, user_id: uuid::Uuid, authenticated: bool) -> Self {
        self.user_id = Some(user_id);
        self.user_authenticated = authenticated;
        self
    }

    pub fn with_thread(mut self, thread_id: uuid::Uuid) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    pub fn with_channel(mut self, channel_id: impl Into<String>) -> Self {
        self.channel_id = Some(channel_id.into());
        self
    }

    pub fn with_guild(mut self, guild_id: impl Into<String>) -> Self {
        self.guild_id = Some(guild_id.into());
        self
    }

    pub fn with_custom(mut self, custom: serde_json::Value) -> Self {
        self.custom = custom;
        self
    }
}

pub trait PromptProvider: Send + Sync {
    fn provide_sections(&self, ctx: &PromptContext) -> Vec<PromptSection>;
}

pub struct PromptBuilder {
    sections: Vec<PromptSection>,
    base_system_prompt: Option<String>,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            base_system_prompt: None,
        }
    }

    pub fn base_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.base_system_prompt = Some(prompt.into());
        self
    }

    pub fn add_section(mut self, section: PromptSection) -> Self {
        self.sections.push(section);
        self
    }

    pub fn build(&self, ctx: &PromptContext) -> String {
        let mut parts = Vec::new();

        if let Some(ref base) = self.base_system_prompt {
            parts.push(base.clone());
        }

        let mut sections: Vec<_> = self
            .sections
            .iter()
            .filter(|s| s.should_include(ctx))
            .collect();

        sections.sort_by_key(|s| s.priority);

        for section in sections {
            parts.push(format!("\n\n{}\n{}", section.name, section.content));
        }

        let context_str = format!("\n\nCONTEXT\n{}\nEND CONTEXT", ctx.platform.as_str());
        parts.push(context_str);

        parts.join("\n")
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}
