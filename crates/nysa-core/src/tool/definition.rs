use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    Object,
    Array,
    String,
    Number,
    Integer,
    Boolean,
    Null,
}

impl Default for SchemaType {
    fn default() -> Self {
        Self::Object
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum PropertyType {
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enum_values: Option<Vec<String>>,
    },
    Integer {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        minimum: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        maximum: Option<i64>,
    },
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        items: Box<PropertyType>,
    },
    Object {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        properties: BTreeMap<String, PropertyType>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        required: Vec<String>,
    },
    Null,
}

impl PropertyType {
    pub fn string() -> Self {
        PropertyType::String {
            description: None,
            enum_values: None,
        }
    }

    pub fn integer() -> Self {
        PropertyType::Integer {
            description: None,
            minimum: None,
            maximum: None,
        }
    }

    pub fn number() -> Self {
        PropertyType::Number {
            description: None,
            minimum: None,
            maximum: None,
        }
    }

    pub fn boolean() -> Self {
        PropertyType::Boolean { description: None }
    }

    pub fn array(items: PropertyType) -> Self {
        PropertyType::Array {
            description: None,
            items: Box::new(items),
        }
    }

    pub fn object() -> Self {
        PropertyType::Object {
            description: None,
            properties: BTreeMap::new(),
            required: Vec::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        match &mut self {
            PropertyType::String { description, .. } => *description = Some(desc.into()),
            PropertyType::Integer { description, .. } => *description = Some(desc.into()),
            PropertyType::Number { description, .. } => *description = Some(desc.into()),
            PropertyType::Boolean { description } => *description = Some(desc.into()),
            PropertyType::Array { description, .. } => *description = Some(desc.into()),
            PropertyType::Object { description, .. } => *description = Some(desc.into()),
            PropertyType::Null => {}
        }
        self
    }

    pub fn enum_values(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        if let PropertyType::String { enum_values, .. } = &mut self {
            *enum_values = Some(values.into_iter().map(Into::into).collect());
        }
        self
    }

    pub fn minimum(mut self, min: impl Into<f64>) -> Self {
        match &mut self {
            PropertyType::Integer { minimum, .. } => *minimum = Some(min.into() as i64),
            PropertyType::Number { minimum, .. } => *minimum = Some(min.into()),
            _ => {}
        }
        self
    }

    pub fn maximum(mut self, max: impl Into<f64>) -> Self {
        match &mut self {
            PropertyType::Integer { maximum, .. } => *maximum = Some(max.into() as i64),
            PropertyType::Number { maximum, .. } => *maximum = Some(max.into()),
            _ => {}
        }
        self
    }

    pub fn property(mut self, name: impl Into<String>, prop: PropertyType) -> Self {
        if let PropertyType::Object { properties, .. } = &mut self {
            properties.insert(name.into(), prop);
        }
        self
    }

    pub fn required(mut self, name: impl Into<String>) -> Self {
        if let PropertyType::Object { required, .. } = &mut self {
            required.push(name.into());
        }
        self
    }

    pub fn items(mut self, items: PropertyType) -> Self {
        if let PropertyType::Array { items: inner, .. } = &mut self {
            *inner = Box::new(items);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "type")]
    pub schema_type: SchemaType,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropertyType>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

impl Default for Schema {
    fn default() -> Self {
        Self {
            schema_type: SchemaType::Object,
            properties: BTreeMap::new(),
            required: Vec::new(),
        }
    }
}

pub struct SchemaBuilder {
    schema: Schema,
}

impl SchemaBuilder {
    pub fn new() -> Self {
        Self {
            schema: Schema::default(),
        }
    }

    pub fn object() -> Self {
        Self::new()
    }

    pub fn property(mut self, name: impl Into<String>, prop: PropertyType) -> Self {
        self.schema.properties.insert(name.into(), prop);
        self
    }

    pub fn required(mut self, name: impl Into<String>) -> Self {
        self.schema.required.push(name.into());
        self
    }

    pub fn build(self) -> Schema {
        self.schema
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.schema).unwrap_or(serde_json::Value::Null)
    }
}

impl Default for SchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Schema,
    pub category: String,
}

pub struct ToolDefinitionBuilder {
    name: Option<String>,
    description: Option<String>,
    parameters: Schema,
    category: String,
}

impl ToolDefinitionBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            description: None,
            parameters: Schema::default(),
            category: "general".to_string(),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn parameters(mut self, schema: Schema) -> Self {
        self.parameters = schema;
        self
    }

    pub fn parameters_builder(mut self, builder: SchemaBuilder) -> Self {
        self.parameters = builder.build();
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn build(self) -> anyhow::Result<ToolDefinition> {
        Ok(ToolDefinition {
            name: self
                .name
                .ok_or_else(|| anyhow::anyhow!("Tool name is required"))?,
            description: self.description.unwrap_or_default(),
            parameters: self.parameters,
            category: self.category,
        })
    }
}

impl Default for ToolDefinitionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDefinition {
    pub fn builder() -> ToolDefinitionBuilder {
        ToolDefinitionBuilder::new()
    }

    pub fn to_openai_tool(&self) -> async_openai::types::ChatCompletionTool {
        async_openai::types::ChatCompletionTool {
            r#type: async_openai::types::ChatCompletionToolType::Function,
            function: async_openai::types::FunctionObject {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: Some(
                    serde_json::to_value(&self.parameters).unwrap_or(serde_json::Value::Null),
                ),
                strict: None,
            },
        }
    }
}
