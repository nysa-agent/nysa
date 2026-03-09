use std::any::Any;

use nysa_core::async_trait;
use nysa_core::{
    App, Extension, ExtensionDef, ExtensionError, ExtensionFactoryRegistry, PropertyType,
    SchemaBuilder, ToolDefinition, ToolError, ToolHandler, ToolResult,
};
use sea_orm::Database;
use serde::{Deserialize, Serialize};

struct EchoHandler;

#[async_trait]
impl ToolHandler for EchoHandler {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("no message");
        Ok(ToolResult::success(format!("Echo: {}", message)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleConfig {
    pub prefix: String,
    pub enabled: bool,
}

pub struct ExampleExtension {
    config: ExampleConfig,
}

impl ExampleExtension {
    fn new(config: ExampleConfig) -> Self {
        Self { config }
    }
}

impl ExtensionDef for ExampleExtension {
    type Config = ExampleConfig;

    fn extension_name() -> &'static str {
        "example"
    }

    fn extension_description() -> Option<&'static str> {
        Some("An example extension demonstrating the tool system")
    }

    fn create(config: Self::Config) -> Self {
        Self::new(config)
    }
}

#[async_trait]
impl Extension for ExampleExtension {
    fn name(&self) -> &'static str {
        "example"
    }

    fn description(&self) -> Option<&'static str> {
        Some("An example extension demonstrating the tool system")
    }

    fn register_tools(&self, registry: &mut nysa_core::ToolRegistry) {
        if !self.config.enabled {
            return;
        }

        let echo_tool = ToolDefinition::builder()
            .name(&format!("{}_echo", self.config.prefix.to_lowercase()))
            .description("Echo back the provided message")
            .parameters(
                SchemaBuilder::object()
                    .property(
                        "message",
                        PropertyType::string().description("The message to echo back"),
                    )
                    .required("message")
                    .build(),
            )
            .category("utility")
            .build()
            .expect("Failed to build echo tool definition");

        registry.register(echo_tool, EchoHandler);
    }

    async fn on_start(&self) -> Result<(), ExtensionError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_url = "postgres://nysa:test-password@localhost:5432/nysa";
    let db = Database::connect(db_url).await?;

    // Method 1: Direct instantiation
    let config = ExampleConfig {
        prefix: "example".to_string(),
        enabled: true,
    };
    let app = App::builder(db)
        .extension(ExampleExtension::new(config))
        .build()
        .await?;

    println!("Registered tools:");
    {
        let registry = app.tool_registry();
        let registry = registry.read().await;
        for tool in registry.all() {
            println!("  • {} ({}) - {}", tool.name, tool.category, tool.description);
        }
        println!("\nCategories: {:?}", registry.categories());
    }

    // Method 2: Factory-based (for Nysa Cloud)
    println!("\n--- Factory Demo ---");
    let mut factory = ExtensionFactoryRegistry::new();
    factory.register::<ExampleExtension>();

    println!("Known extensions: {:?}", factory.known_extensions());
    println!(
        "Extension descriptions: {:?}",
        factory.extension_descriptions()
    );

    let config_json = serde_json::json!({
        "prefix": "cloud",
        "enabled": true
    });

    if let Some(_ext) = factory.create_or_warn("example", config_json) {
        println!("Successfully created extension via factory!");
    }

    // Unknown extension - logs warning
    factory.create_or_warn("unknown", serde_json::json!({}));

    Ok(())
}
