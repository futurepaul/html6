use crate::parser::{frontmatter, mdx};
use crate::runtime::ComponentRegistry;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Load and parse a .hnmd file, returning both document and component registry
pub fn load_hnmd(path: &str) -> Result<(crate::parser::ast::Document, ComponentRegistry)> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path))?;

    let doc = parse_hnmd(&content)?;

    // Create component registry with base path from document location
    let base_path = Path::new(path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut registry = ComponentRegistry::new(base_path);

    // Load all imported components
    for (name, import_path) in &doc.imports {
        registry.load_component(name, import_path)
            .map_err(|e| anyhow::anyhow!("Failed to load component '{}' from '{}': {}", name, import_path, e))?;
    }

    Ok((doc, registry))
}

/// Parse HNMD content (frontmatter + markdown)
pub fn parse_hnmd(content: &str) -> Result<crate::parser::ast::Document> {
    // Check if content starts with ---
    let has_frontmatter = content.trim_start().starts_with("---");

    let (frontmatter_str, body_str) = if has_frontmatter {
        // Split on --- delimiters
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() == 3 {
            // Has frontmatter: empty, frontmatter, body
            (parts[1].trim(), parts[2].trim())
        } else {
            return Err(anyhow::anyhow!("Incomplete frontmatter (missing closing ---)"));
        }
    } else {
        // No frontmatter, entire content is body
        ("", content.trim())
    };

    // Parse frontmatter (or use empty)
    let frontmatter = if frontmatter_str.is_empty() {
        crate::parser::ast::Frontmatter::new()
    } else {
        frontmatter::parse_frontmatter(frontmatter_str)?
    };

    // Parse imports from frontmatter
    let imports = if frontmatter_str.is_empty() {
        std::collections::HashMap::new()
    } else {
        frontmatter::parse_imports(frontmatter_str)?
    };

    // Parse markdown body with MDX
    let body = mdx::parse_body(body_str)?;

    let mut doc = crate::parser::ast::Document::new(frontmatter, body);
    doc.imports = imports;

    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Node;
    use crate::renderer::RenderContext;
    use crate::runtime::RuntimeContext;

    #[test]
    fn test_parse_markdown_only() {
        let content = "# Hello\n\nWorld";
        let doc = parse_hnmd(content).unwrap();

        assert_eq!(doc.body.len(), 2); // heading + paragraph
        assert_eq!(doc.frontmatter.filters.len(), 0);
    }

    #[test]
    fn test_parse_with_frontmatter() {
        let content = r#"---
filters:
  feed:
    kinds: [1]
    limit: 10
---

# My App

Content here"#;

        let doc = parse_hnmd(content).unwrap();

        assert_eq!(doc.frontmatter.filters.len(), 1);
        assert!(doc.frontmatter.filters.contains_key("feed"));
        assert_eq!(doc.body.len(), 2);
    }

    #[test]
    fn test_load_hello_hnmd() {
        let (doc, registry) = load_hnmd("apps/hello.hnmd").unwrap();

        // Should have parsed the markdown
        assert!(doc.body.len() > 0);

        // Should have no components
        assert_eq!(registry.list_components().len(), 0);

        // Debug: print the parsed structure
        println!("Parsed document body:");
        for (i, node) in doc.body.iter().enumerate() {
            println!("{}: {:?}", i, node);
        }
    }

    #[test]
    fn test_load_component_imports() {
        let (doc, registry) = load_hnmd("apps/test_component.hnmd").unwrap();

        // Should have loaded Profile component
        assert_eq!(registry.list_components().len(), 1);
        assert!(registry.contains("Profile"));

        // Document should have import
        assert!(doc.imports.contains_key("Profile"));

        // Should have CustomComponent node in body
        let has_custom = doc.body.iter().any(|n| matches!(n, Node::CustomComponent { .. }));
        assert!(has_custom, "Document should contain CustomComponent node");

        println!("✅ Component loading test passed!");
        println!("   Loaded components: {:?}", registry.list_components());
    }

    #[test]
    fn test_parse_vstack_with_components() {
        let content = r#"
<vstack>

## Components

<button label="Click Me" />

<input name="message" placeholder="Type something..." />

</vstack>
"#;
        let doc = parse_hnmd(content).unwrap();

        println!("Parsed VStack test:");
        for (i, node) in doc.body.iter().enumerate() {
            println!("{}: {:?}", i, node);
        }

        // Should have at least one VStack
        assert!(doc.body.iter().any(|n| matches!(n, crate::parser::ast::Node::VStack { .. })));
    }

    #[test]
    fn test_expression_parsing_and_evaluation() {
        let content = r#"---
state:
  appName: "HNMD Demo"
  version: "0.1.0"
  count: 42
---

# {state.appName}

**Version:** {state.version}
**Count:** {state.count}
"#;

        let doc = parse_hnmd(content).expect("Failed to parse");

        // Debug: print what we actually got
        println!("\n=== Parsed AST ===");
        for (i, node) in doc.body.iter().enumerate() {
            println!("Node {}: {:?}", i, node);
        }

        // First node should be a heading
        assert!(matches!(doc.body[0], Node::Heading { .. }), "First node should be a heading");

        if let Node::Heading { children, .. } = &doc.body[0] {
            // Should have at least one child
            assert!(!children.is_empty(), "Heading should have children");

            println!("\nHeading children: {:?}", children);

            // Look for Expr node
            let has_expr = children.iter().any(|child| matches!(child, Node::Expr { .. }));
            assert!(has_expr, "Heading should contain an Expr node for {{state.appName}}, but got: {:?}", children);

            if let Some(Node::Expr { expression }) = children.iter().find(|c| matches!(c, Node::Expr { .. })) {
                assert_eq!(expression, "state.appName");

                // Now test evaluation
                let runtime_ctx = RuntimeContext::with_state(doc.frontmatter.state.clone());
                let mut render_ctx = RenderContext::new(runtime_ctx);

                let result = render_ctx.eval(expression).expect("Should evaluate expression");
                assert_eq!(result, serde_json::json!("HNMD Demo"));
            }
        }
    }
}
