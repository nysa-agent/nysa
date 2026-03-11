use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::tool::definition::ToolDefinition;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("{0}")]
    Custom(String),
}

impl From<&str> for ToolError {
    fn from(msg: &str) -> Self {
        ToolError::Custom(msg.to_string())
    }
}

impl From<String> for ToolError {
    fn from(msg: String) -> Self {
        ToolError::Custom(msg)
    }
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError>;
}

pub type BoxedToolHandler = Box<dyn ToolHandler>;

struct RegisteredTool {
    definition: ToolDefinition,
    handler: BoxedToolHandler,
}

pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
    categories: HashMap<String, Vec<String>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    pub fn register<H>(&mut self, definition: ToolDefinition, handler: H)
    where
        H: ToolHandler + 'static,
    {
        let name = definition.name.clone();
        let category = definition.category.clone();

        self.categories
            .entry(category)
            .or_default()
            .push(name.clone());

        let handler_ptr: Box<dyn ToolHandler> = Box::new(handler);

        self.tools.insert(
            name,
            RegisteredTool {
                definition,
                handler: handler_ptr,
            },
        );
    }

    pub fn register_boxed(&mut self, definition: ToolDefinition, handler: BoxedToolHandler) {
        let name = definition.name.clone();
        let category = definition.category.clone();

        self.categories
            .entry(category)
            .or_default()
            .push(name.clone());

        self.tools.insert(
            name,
            RegisteredTool {
                definition,
                handler,
            },
        );
    }

    pub fn get(&self, name: &str) -> Option<(&ToolDefinition, &dyn ToolHandler)> {
        self.tools
            .get(name)
            .map(|t| (&t.definition, t.handler.as_ref()))
    }

    pub fn get_definition(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name).map(|t| &t.definition)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn by_category(&self, category: &str) -> Vec<&ToolDefinition> {
        self.categories
            .get(category)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.get_definition(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn categories(&self) -> Vec<&str> {
        self.categories.keys().map(|s| s.as_str()).collect()
    }

    pub fn all(&self) -> Vec<&ToolDefinition> {
        self.tools.values().map(|t| &t.definition).collect()
    }

    pub fn all_handlers(&self) -> Vec<(&ToolDefinition, &dyn ToolHandler)> {
        self.tools
            .values()
            .map(|t| (&t.definition, t.handler.as_ref()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn to_openai_tools(&self) -> Vec<async_openai::types::ChatCompletionTool> {
        self.tools
            .values()
            .map(|t| t.definition.to_openai_tool())
            .collect()
    }

    pub fn remove(&mut self, name: &str) -> Option<ToolDefinition> {
        let removed = self.tools.remove(name);
        removed.map(|t| {
            if let Some(tools_in_cat) = self.categories.get_mut(&t.definition.category) {
                tools_in_cat.retain(|n| n != name);
                if tools_in_cat.is_empty() {
                    self.categories.remove(&t.definition.category);
                }
            }
            t.definition
        })
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ToolExecutor {
    registry: std::sync::Arc<tokio::sync::RwLock<ToolRegistry>>,
}

impl ToolExecutor {
    pub fn new(registry: std::sync::Arc<tokio::sync::RwLock<ToolRegistry>>) -> Self {
        Self { registry }
    }

    pub async fn dispatch(&self, tool_name: &str, args: Value) -> Result<ToolResult, ToolError> {
        let registry = self.registry.read().await;

        let (_, handler) = registry
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound(tool_name.to_string()))?;

        handler.execute(args).await
    }

    pub async fn dispatch_tool_call(
        &self,
        tool_call: &async_openai::types::ChatCompletionMessageToolCall,
    ) -> ToolResult {
        let args: Value =
            serde_json::from_str(&tool_call.function.arguments).unwrap_or(Value::Null);

        match self.dispatch(&tool_call.function.name, args).await {
            Ok(result) => result,
            Err(e) => ToolResult::error(format!("Tool execution error: {}", e)),
        }
    }
}
