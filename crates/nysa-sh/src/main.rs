use nysa_core::{
    App,
    config::{AiConfigBuilder, ChatConfigBuilder, Config, EmbeddingConfigBuilder, ExtensionConfig},
};
use sea_orm::Database;

#[allow(dead_code)]
struct MyExtensionConfig {
    pub enabled: bool,
    pub api_key: String,
}

impl ExtensionConfig for MyExtensionConfig {
    fn extension_name(&self) -> &'static str {
        "my_extension"
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_url = "postgres://nysa:test-password@localhost:5432/nysa";
    let db = Database::connect(db_url).await?;

    let openrouter_api_key =
        std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY must be set");

    let config = Config::builder()
        .ai(
            AiConfigBuilder::new()
                .chat(
                    ChatConfigBuilder::new()
                        .base_url("https://openrouter.ai/api/v1")
                        .api_key(&openrouter_api_key)
                        .model("moonshotai/kimi-k2.5")
                        .temperature(0.7)
                        .max_completion_tokens(4096)
                        .build()?,
                )
                .embedding(
                    EmbeddingConfigBuilder::new()
                        .base_url("https://openrouter.ai/api/v1")
                        .api_key(&openrouter_api_key)
                        .model("text-embedding-3-small")
                        .dimensions(1536)
                        .build()?,
                )
                .build()?,
        )
        .extension(MyExtensionConfig {
            enabled: true,
            api_key: "ext-key".to_string(),
        })
        .build();

    let _app = App::builder(db).with_config(config).build().await?;

    Ok(())
}
