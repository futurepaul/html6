use crate::parser::{frontmatter, mdx};
use anyhow::{Context, Result};
use std::fs;

/// Load and parse a .hnmd file
pub fn load_hnmd(path: &str) -> Result<crate::parser::ast::Document> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path))?;

    parse_hnmd(&content)
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

    // Parse markdown body with MDX
    let body = mdx::parse_body(body_str)?;

    Ok(crate::parser::ast::Document::new(frontmatter, body))
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
        let doc = load_hnmd("apps/hello.hnmd").unwrap();

        // Should have parsed the markdown
        assert!(doc.body.len() > 0);

        // Debug: print the parsed structure
        println!("Parsed document body:");
        for (i, node) in doc.body.iter().enumerate() {
            println!("{}: {:?}", i, node);
        }
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
