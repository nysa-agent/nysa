use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool::definition::{PropertyType, SchemaBuilder, ToolDefinition};
use crate::tool::registry::{ToolError, ToolHandler, ToolResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDetail {
    NamesOnly,
    Summaries,
    FullDefinitions,
}

impl Default for SearchDetail {
    fn default() -> Self {
        Self::Summaries
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchArgs {
    pub query: Option<String>,
    pub category: Option<String>,
}

pub struct ToolSearchHandler {
    detail: SearchDetail,
}

impl ToolSearchHandler {
    pub fn new(detail: SearchDetail) -> Self {
        Self { detail }
    }
}

#[async_trait]
impl ToolHandler for ToolSearchHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let search_args: ToolSearchArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
        
        let _ = (search_args.query, search_args.category);
        
        let content = match self.detail {
            SearchDetail::NamesOnly => {
                "Tool search (names only) - registry access required at runtime".to_string()
            }
            SearchDetail::Summaries => {
                "Tool search (summaries) - registry access required at runtime".to_string()
            }
            SearchDetail::FullDefinitions => {
                "Tool search (full definitions) - registry access required at runtime".to_string()
            }
        };
        
        Ok(ToolResult::success(content))
    }
}

pub fn create_tool_search_tool(detail: SearchDetail) -> (ToolDefinition, ToolSearchHandler) {
    let parameters = SchemaBuilder::object()
        .property(
            "query",
            PropertyType::string()
                .description("Search query to filter tools by name or description"),
        )
        .property(
            "category",
            PropertyType::string()
                .description("Filter tools by category"),
        )
        .build();
    
    let description = match detail {
        SearchDetail::NamesOnly => "Search available tools and return their names",
        SearchDetail::Summaries => "Search available tools and return their names and descriptions",
        SearchDetail::FullDefinitions => "Search available tools and return their full definitions including parameters",
    };
    
    let definition = ToolDefinition::builder()
        .name("tool_search")
        .description(description)
        .parameters(parameters)
        .category("system")
        .build()
        .expect("Failed to build tool_search definition");
    
    let handler = ToolSearchHandler::new(detail);
    
    (definition, handler)
}

pub struct DynamicToolSearchHandler;

impl DynamicToolSearchHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolHandler for DynamicToolSearchHandler {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let search_args: ToolSearchArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
        
        let _ = (search_args.query, search_args.category);
        
        Ok(ToolResult::success(
            "Dynamic tool search - requires registry context at runtime"
        ))
    }
}

impl Default for DynamicToolSearchHandler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn tool_search_definition() -> ToolDefinition {
    let parameters = SchemaBuilder::object()
        .property(
            "query",
            PropertyType::string()
                .description("Search query to filter tools by name or description"),
        )
        .property(
            "category",
            PropertyType::string()
                .description("Filter tools by category"),
        )
        .property(
            "detail",
            PropertyType::string()
                .description("Level of detail: 'names', 'summaries', or 'full'")
                .enum_values(["names", "summaries", "full"]),
        )
        .build();
    
    ToolDefinition::builder()
        .name("tool_search")
        .description("Search available tools by query and/or category. Returns tool information at the specified detail level.")
        .parameters(parameters)
        .category("system")
        .build()
        .expect("Failed to build tool_search definition")
}
