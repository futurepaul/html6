use crate::parser::component_def::{parse_component, ComponentDef};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Registry for component definitions
#[derive(Clone)]
pub struct ComponentRegistry {
    /// Loaded components by name
    components: HashMap<String, ComponentDef>,
    /// Base path for resolving relative component imports
    base_path: PathBuf,
}

impl ComponentRegistry {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            components: HashMap::new(),
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Load a component from a file path
    pub fn load_component(&mut self, name: impl Into<String>, path: impl AsRef<str>) -> Result<()> {
        let name = name.into();
        let path_str = path.as_ref();

        // Resolve relative path
        let full_path = if path_str.starts_with("./") || path_str.starts_with("../") {
            self.base_path.join(path_str)
        } else {
            PathBuf::from(path_str)
        };

        // Read file
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read component file {}", full_path.display()))?;

        // Parse component
        let component_def = parse_component(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse component: {}", e))?;

        // Recursively load nested component imports
        for (nested_name, nested_path) in &component_def.imports {
            if !self.components.contains_key(nested_name) {
                // Resolve nested import relative to current component's directory
                let nested_base = full_path.parent().unwrap_or(&self.base_path);
                let resolved_nested = if nested_path.starts_with("./") || nested_path.starts_with("../") {
                    nested_base.join(nested_path)
                } else {
                    PathBuf::from(nested_path)
                };

                let nested_content = fs::read_to_string(&resolved_nested)
                    .with_context(|| format!("Failed to read nested component {}", resolved_nested.display()))?;
                let nested_def = parse_component(&nested_content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse nested component: {}", e))?;
                self.components.insert(nested_name.clone(), nested_def);
            }
        }

        // Store component
        self.components.insert(name, component_def);
        Ok(())
    }

    /// Get a component by name
    pub fn get(&self, name: &str) -> Option<&ComponentDef> {
        self.components.get(name)
    }

    /// Check if a component exists
    pub fn contains(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    /// List all registered component names
    pub fn list_components(&self) -> Vec<&str> {
        self.components.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_registry() {
        let mut registry = ComponentRegistry::new(".");

        // This test is placeholder - full test requires actual .hnmc files
        assert_eq!(registry.list_components().len(), 0);
        assert!(!registry.contains("Profile"));
    }
}
