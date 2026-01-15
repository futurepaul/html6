use crate::parser::ast::{Filter, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Component definition from .hnmc file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    /// Component imports (name → file path)
    #[serde(default)]
    pub imports: HashMap<String, String>,
    /// Component-scoped queries (Nostr filters)
    #[serde(default)]
    pub queries: HashMap<String, Filter>,
    /// Props schema
    #[serde(default)]
    pub props: HashMap<String, PropSchema>,
    /// Component body (markup)
    pub body: Vec<Node>,
}

/// Schema for a component prop
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropSchema {
    /// Type name: "string", "number", "boolean", etc.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Whether the prop is required
    #[serde(default)]
    pub required: bool,
    /// Default value if not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl ComponentDef {
    pub fn new(body: Vec<Node>) -> Self {
        Self {
            imports: HashMap::new(),
            queries: HashMap::new(),
            props: HashMap::new(),
            body,
        }
    }

    pub fn with_query(mut self, id: impl Into<String>, query: Filter) -> Self {
        self.queries.insert(id.into(), query);
        self
    }

    pub fn with_prop(mut self, name: impl Into<String>, type_name: impl Into<String>) -> Self {
        self.props.insert(
            name.into(),
            PropSchema {
                type_name: type_name.into(),
                required: false,
                default: None,
            },
        );
        self
    }

    pub fn with_import(mut self, name: impl Into<String>, path: impl Into<String>) -> Self {
        self.imports.insert(name.into(), path.into());
        self
    }
}

/// Parse a .hnmc component file
pub fn parse_component(content: &str) -> Result<ComponentDef, String> {
    // Split frontmatter and body (same logic as loader.rs)
    let has_frontmatter = content.trim_start().starts_with("---");

    let (frontmatter_str, body_str) = if has_frontmatter {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() == 3 {
            (parts[1].trim(), parts[2].trim())
        } else {
            return Err("Incomplete frontmatter (missing closing ---)".to_string());
        }
    } else {
        ("", content.trim())
    };

    // Parse frontmatter YAML
    let frontmatter_yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&frontmatter_str)
        .map_err(|e| format!("Failed to parse component frontmatter YAML: {}", e))?;

    // Extract imports
    let imports = frontmatter_yaml
        .get("imports")
        .and_then(|v| serde_yaml_ng::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Extract queries (same as filters in documents)
    let queries = frontmatter_yaml
        .get("queries")
        .and_then(|v| serde_yaml_ng::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Extract props schema
    let props = frontmatter_yaml
        .get("props")
        .and_then(|v| {
            // Props can be defined as "propName: type" or with full schema
            if let Some(obj) = v.as_mapping() {
                let mut prop_map = HashMap::new();
                for (key, value) in obj {
                    if let Some(key_str) = key.as_str() {
                        let schema = if let Some(type_str) = value.as_str() {
                            // Simple form: "pubkey: string"
                            PropSchema {
                                type_name: type_str.to_string(),
                                required: false,
                                default: None,
                            }
                        } else {
                            // Complex form with schema object
                            serde_yaml_ng::from_value(value.clone()).unwrap_or(PropSchema {
                                type_name: "any".to_string(),
                                required: false,
                                default: None,
                            })
                        };
                        prop_map.insert(key_str.to_string(), schema);
                    }
                }
                Some(prop_map)
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Parse body markdown/components
    let body = crate::parser::mdx::parse_body(&body_str)
        .map_err(|e| format!("Failed to parse component body: {}", e))?;

    Ok(ComponentDef {
        imports,
        queries,
        props,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_component() {
        let content = r#"---
props:
  pubkey: string

queries:
  metadata:
    kinds: [0]
    authors: ["{props.pubkey}"]
    limit: 1
---

**{queries.metadata[0].content | fromjson | .name // "Anonymous"}**
"#;

        let component = parse_component(content).unwrap();

        assert_eq!(component.props.len(), 1);
        assert!(component.props.contains_key("pubkey"));
        assert_eq!(component.props["pubkey"].type_name, "string");

        assert_eq!(component.queries.len(), 1);
        assert!(component.queries.contains_key("metadata"));

        assert!(!component.body.is_empty());
    }

    #[test]
    fn test_component_def_builder() {
        let component = ComponentDef::new(vec![])
            .with_prop("limit", "number")
            .with_query("notes", Filter::new().kinds(vec![1]))
            .with_import("Profile", "./Profile.hnmc");

        assert_eq!(component.props.len(), 1);
        assert_eq!(component.queries.len(), 1);
        assert_eq!(component.imports.len(), 1);
    }
}
