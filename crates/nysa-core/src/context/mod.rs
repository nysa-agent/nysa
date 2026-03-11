use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    DiscordGuild,
    DiscordDm,
    Minecraft,
    Cli,
    // Api,
    Custom { name: String },
}

impl Platform {
    pub fn as_str(&self) -> &str {
        match self {
            Platform::DiscordGuild => "discord_guild",
            Platform::DiscordDm => "discord_dm",
            Platform::Minecraft => "minecraft",
            Platform::Cli => "cli",
            // Platform::Api => "api",
            Platform::Custom { name } => name,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "discord_guild" => Platform::DiscordGuild,
            "discord_dm" => Platform::DiscordDm,
            "minecraft" => Platform::Minecraft,
            "cli" => Platform::Cli,
            // "api" => Platform::Api,
            other => Platform::Custom {
                name: other.to_string(),
            },
        }
    }
}

impl Default for Platform {
    fn default() -> Self {
        Platform::Cli
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformDetails {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub internal_id: Uuid,
    pub platform_id: String,
    pub platform: Platform,
    pub display_name: String,
}

impl UserContext {
    pub fn new(
        internal_id: Uuid,
        platform_id: String,
        platform: Platform,
        display_name: String,
    ) -> Self {
        Self {
            internal_id,
            platform_id,
            platform,
            display_name,
        }
    }

    pub fn format_tag(&self) -> String {
        let platform_str = match self.platform {
            Platform::DiscordGuild | Platform::DiscordDm => "discord",
            Platform::Minecraft => "minecraft",
            Platform::Cli => "cli",
            // Platform::Api => "api",
            Platform::Custom { ref name } => name,
        };

        format!(
            "[user:{}@{}:{}]",
            self.internal_id, platform_str, self.platform_id
        )
    }

    pub fn anonymous(platform: Platform, platform_id: String, display_name: String) -> Self {
        Self {
            internal_id: Uuid::nil(),
            platform_id,
            platform,
            display_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContext {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub platform: Platform,
    pub user: Option<UserContext>,
    pub thread_id: Option<Uuid>,
    pub is_reply: bool,
    pub reply_to_message_id: Option<Uuid>,
    pub channel_id: Option<String>,
}

impl MessageContext {
    pub fn new(platform: Platform) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            platform,
            user: None,
            thread_id: None,
            is_reply: false,
            reply_to_message_id: None,
            channel_id: None,
        }
    }

    pub fn with_user(mut self, user: UserContext) -> Self {
        self.user = Some(user);
        self
    }

    pub fn with_thread(mut self, thread_id: Uuid) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    pub fn with_reply_to(mut self, message_id: Uuid) -> Self {
        self.is_reply = true;
        self.reply_to_message_id = Some(message_id);
        self
    }

    pub fn with_channel(mut self, channel_id: String) -> Self {
        self.channel_id = Some(channel_id);
        self
    }

    pub fn format_for_llm(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "<timestamp>{}</timestamp>",
            self.timestamp.to_rfc3339()
        ));

        let platform_str = match self.platform {
            Platform::DiscordGuild => "discord_guild",
            Platform::DiscordDm => "discord_dm",
            Platform::Minecraft => "minecraft",
            Platform::Cli => "cli",
            // Platform::Api => "api",
            Platform::Custom { ref name } => name,
        };
        parts.push(format!("<platform>{}</platform>", platform_str));

        if let Some(ref channel) = self.channel_id {
            parts.push(format!("<channel>{}</channel>", channel));
        }

        if let Some(ref user) = self.user {
            parts.push(user.format_tag());
        }

        if let Some(thread_id) = self.thread_id {
            parts.push(format!("<thread_id>{}</thread_id>", thread_id));
        }

        if self.is_reply {
            parts.push("<is_reply>true</is_reply>".to_string());
            if let Some(reply_to) = self.reply_to_message_id {
                parts.push(format!("<reply_to>{}</reply_to>", reply_to));
            }
        }

        parts.join("\n")
    }
}

pub fn format_system_context(
    platform: Platform,
    user: Option<&UserContext>,
    thread_id: Option<Uuid>,
    channel_id: Option<&str>,
) -> String {
    let ctx = MessageContext::new(platform)
        .with_thread(thread_id.unwrap_or(Uuid::nil()))
        .with_channel(channel_id.unwrap_or("unknown").to_string());

    let ctx = match user {
        Some(u) => ctx.with_user(u.clone()),
        None => ctx,
    };

    ctx.format_for_llm()
}
