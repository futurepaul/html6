use crate::parser::ast::{ListItem, Node};
use anyhow::Result;
use markdown::mdast;
use regex::Regex;

/// Parse markdown body with MDX JSX support
pub fn parse_body(source: &str) -> Result<Vec<Node>> {
    let mut options = markdown::ParseOptions::default();

    // Enable MDX JSX parsing (disable HTML parsing as it conflicts)
    options.constructs.mdx_jsx_flow = true;  // Block-level JSX components
    options.constructs.mdx_jsx_text = true;  // Inline JSX components
    options.constructs.mdx_expression_flow = true;  // Block-level expressions {expr}
    options.constructs.mdx_expression_text = true;  // Inline expressions {expr}
    options.constructs.html_flow = false;    // Must disable when using MDX JSX
    options.constructs.html_text = false;    // Must disable when using MDX JSX

    let ast = markdown::to_mdast(source, &options)
        .map_err(|e| anyhow::anyhow!("Failed to parse markdown: {}", e))?;

    // The root is always a Root node containing children
    match ast {
        mdast::Node::Root(root) => transform_children(root.children),
        _ => Ok(vec![]),
    }
}

/// Transform a list of markdown AST children
fn transform_children(children: Vec<mdast::Node>) -> Result<Vec<Node>> {
    children
        .into_iter()
        .flat_map(|child| transform_node(child).transpose())
        .collect()
}

/// Transform a single markdown node to our AST
fn transform_node(node: mdast::Node) -> Result<Option<Node>> {
    Ok(Some(match node {
        // Markdown nodes
        mdast::Node::Heading(h) => Node::Heading {
            level: h.depth,
            children: transform_children(h.children)?,
        },

        mdast::Node::Paragraph(p) => {
            // Check if paragraph contains only an image - render directly
            if p.children.len() == 1 {
                if let mdast::Node::Image(_) = &p.children[0] {
                    return transform_node(p.children[0].clone());
                }
            }

            Node::Paragraph {
                children: transform_children(p.children)?,
            }
        },

        mdast::Node::Text(t) => {
            // With MDX enabled, expressions are parsed as MdxTextExpression
            // So Text nodes are just plain text
            Node::Text { value: t.value }
        }

        mdast::Node::Strong(s) => Node::Strong {
            children: transform_children(s.children)?,
        },

        mdast::Node::Emphasis(e) => Node::Emphasis {
            children: transform_children(e.children)?,
        },

        mdast::Node::List(l) => Node::List {
            ordered: l.ordered,
            items: l
                .children
                .into_iter()
                .map(|item| {
                    if let mdast::Node::ListItem(li) = item {
                        Ok(ListItem {
                            children: transform_children(li.children)?,
                        })
                    } else {
                        Err(anyhow::anyhow!("Expected ListItem in List"))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        },

        mdast::Node::Link(link) => Node::Link {
            url: link.url,
            children: transform_children(link.children)?,
        },

        mdast::Node::Image(img) => Node::Image {
            src: img.url,
            alt: img.alt,
        },

        mdast::Node::ThematicBreak(_) => Node::Spacer { size: Some(20.0) },

        // MDX JSX Components - this is the good stuff!
        mdast::Node::MdxJsxFlowElement(jsx) => {
            return transform_jsx_element(jsx);
        }

        mdast::Node::MdxJsxTextElement(jsx) => {
            return transform_jsx_element_text(jsx);
        }

        // MDX Expressions
        mdast::Node::MdxFlowExpression(expr) => Node::Expr {
            expression: expr.value,
        },

        mdast::Node::MdxTextExpression(expr) => Node::Expr {
            expression: expr.value,
        },

        // Unsupported nodes - skip
        mdast::Node::Code(_) => return Ok(None),
        mdast::Node::InlineCode(code) => Node::Text {
            value: format!("`{}`", code.value),
        },
        mdast::Node::Blockquote(_) => return Ok(None),
        mdast::Node::Table(_) => return Ok(None),
        mdast::Node::Html(_) => return Ok(None), // Shouldn't happen with MDX mode
        mdast::Node::MdxjsEsm(_) => return Ok(None), // ESM imports - not supported yet

        // Catch-all
        _ => return Ok(None),
    }))
}

/// Parse text that might contain {expr} interpolations
fn parse_text_with_expressions(text: &str) -> Result<Option<Node>> {
    let expr_re = Regex::new(r"\{([^}]+)\}").unwrap();

    // Check if text contains expressions
    if expr_re.find(text).is_none() {
        return Ok(Some(Node::Text {
            value: text.to_string(),
        }));
    }

    // Has expressions - this shouldn't happen with MDX mode enabled
    // because MDX handles {expr} natively as MdxTextExpression
    // But just in case, fall back to text
    Ok(Some(Node::Text {
        value: text.to_string(),
    }))
}

/// Transform an MDX JSX flow element (block-level component)
fn transform_jsx_element(jsx: mdast::MdxJsxFlowElement) -> Result<Option<Node>> {
    let tag_name = jsx.name.as_deref().unwrap_or("fragment");

    // Extract attributes into our format
    let mut attrs = std::collections::HashMap::new();
    for attr in jsx.attributes {
        if let mdast::AttributeContent::Property(prop) = attr {
            let value = match prop.value {
                Some(mdast::AttributeValue::Literal(lit)) => crate::parser::component::AttrValue::Literal(lit),
                Some(mdast::AttributeValue::Expression(expr)) => {
                    crate::parser::component::AttrValue::Expression(expr.value)
                }
                None => crate::parser::component::AttrValue::Literal(String::new()),
            };
            attrs.insert(prop.name, value);
        }
    }

    // Transform children
    let children = transform_children(jsx.children)?;

    // Build component node based on tag name
    build_component_from_jsx(tag_name, attrs, children)
}

/// Transform an MDX JSX text element (inline component)
fn transform_jsx_element_text(jsx: mdast::MdxJsxTextElement) -> Result<Option<Node>> {
    let tag_name = jsx.name.as_deref().unwrap_or("fragment");

    // Extract attributes
    let mut attrs = std::collections::HashMap::new();
    for attr in jsx.attributes {
        if let mdast::AttributeContent::Property(prop) = attr {
            let value = match prop.value {
                Some(mdast::AttributeValue::Literal(lit)) => crate::parser::component::AttrValue::Literal(lit),
                Some(mdast::AttributeValue::Expression(expr)) => {
                    crate::parser::component::AttrValue::Expression(expr.value)
                }
                None => crate::parser::component::AttrValue::Literal(String::new()),
            };
            attrs.insert(prop.name, value);
        }
    }

    // Transform children
    let children = transform_children(jsx.children)?;

    // Build component node
    build_component_from_jsx(tag_name, attrs, children)
}

/// Build our AST node from JSX component info
fn build_component_from_jsx(
    tag: &str,
    attrs: std::collections::HashMap<String, crate::parser::component::AttrValue>,
    children: Vec<Node>,
) -> Result<Option<Node>> {
    use crate::parser::component::AttrValue;

    let node = match tag {
        "each" => {
            let from = get_attr_expr(&attrs, "from")?;
            let as_name = get_attr_literal(&attrs, "as")?;
            Node::Each { from, as_name, children }
        }

        "if" => {
            let value = get_attr_expr(&attrs, "value")?;
            // TODO: Handle <else> tag in children
            Node::If {
                value,
                children,
                else_children: None,
            }
        }

        "button" => {
            let on_click = attrs.get("on_click").or_else(|| attrs.get("onClick"))
                .map(|av| match av {
                    AttrValue::Expression(e) => e.clone(),
                    AttrValue::Literal(l) => l.clone(),
                });

            // Get label from attribute or children
            let label_children = if let Some(label_attr) = attrs.get("label") {
                match label_attr {
                    AttrValue::Literal(s) => vec![Node::text(s.clone())],
                    AttrValue::Expression(_) => {
                        return Err(anyhow::anyhow!(
                            "Dynamic button labels not yet supported. Use literal string."
                        ));
                    }
                }
            } else if !children.is_empty() {
                children
            } else {
                return Err(anyhow::anyhow!("Button must have either label attribute or children"));
            };

            Node::Button {
                on_click,
                children: label_children,
            }
        }

        "input" => {
            let name = get_attr_literal(&attrs, "name")?;
            let placeholder = attrs.get("placeholder").map(|av| match av {
                AttrValue::Literal(lit) => lit.clone(),
                AttrValue::Expression(expr) => expr.clone(),
            });

            Node::Input { name, placeholder }
        }

        "vstack" => {
            let flex = attrs.get("flex").and_then(|av| {
                match av {
                    AttrValue::Literal(s) => s.parse::<f64>().ok(),
                    _ => None,
                }
            });

            Node::VStack {
                children,
                width: None,  // TODO: Parse width/height/align
                height: None,
                flex,
                align: None,
            }
        }

        "hstack" => {
            let flex = attrs.get("flex").and_then(|av| {
                match av {
                    AttrValue::Literal(s) => s.parse::<f64>().ok(),
                    _ => None,
                }
            });

            Node::HStack {
                children,
                width: None,
                height: None,
                flex,
                align: None,
            }
        }

        "grid" => {
            let columns = attrs.get("columns").and_then(|av| {
                match av {
                    AttrValue::Literal(s) => s.parse::<usize>().ok(),
                    AttrValue::Expression(e) => e.parse::<usize>().ok(),
                }
            });

            Node::Grid { children, columns }
        }

        "spacer" => {
            let size = attrs.get("size").and_then(|av| {
                match av {
                    AttrValue::Literal(s) => s.parse::<f64>().ok(),
                    _ => None,
                }
            });

            Node::Spacer { size }
        }

        _ => {
            return Err(anyhow::anyhow!("Unknown component: <{}>", tag));
        }
    };

    Ok(Some(node))
}

/// Helper: Get required attribute as expression/literal string
fn get_attr_expr(attrs: &std::collections::HashMap<String, crate::parser::component::AttrValue>, name: &str) -> Result<String> {
    use crate::parser::component::AttrValue;

    attrs.get(name)
        .ok_or_else(|| anyhow::anyhow!("Missing required attribute '{}'", name))
        .map(|av| match av {
            AttrValue::Expression(e) => e.clone(),
            AttrValue::Literal(l) => l.clone(),
        })
}

/// Helper: Get required attribute as literal string only
fn get_attr_literal(attrs: &std::collections::HashMap<String, crate::parser::component::AttrValue>, name: &str) -> Result<String> {
    use crate::parser::component::AttrValue;

    attrs.get(name)
        .ok_or_else(|| anyhow::anyhow!("Missing required attribute '{}'", name))
        .and_then(|av| match av {
            AttrValue::Literal(l) => Ok(l.clone()),
            AttrValue::Expression(e) => Err(anyhow::anyhow!(
                "Attribute '{}' must be a literal string, got expression: {}", name, e
            )),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_component() {
        let md = r#"<button label="Click Me" />"#;
        let nodes = parse_body(md).unwrap();

        assert_eq!(nodes.len(), 1);
        assert!(matches!(&nodes[0], Node::Button { .. }));
    }

    #[test]
    fn test_parse_nested_components() {
        let md = r#"<vstack>
<button label="First" />
<button label="Second" />
</vstack>"#;
        let nodes = parse_body(md).unwrap();

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::VStack { children, .. } => {
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected VStack"),
        }
    }

    #[test]
    fn test_whitespace_normalization() {
        // With and without blank lines should produce same AST
        let with_blanks = r#"<hstack>

<vstack>

Test

</vstack>

</hstack>"#;

        let without_blanks = r#"<hstack>
<vstack>
Test
</vstack>
</hstack>"#;

        let nodes1 = parse_body(with_blanks).unwrap();
        let nodes2 = parse_body(without_blanks).unwrap();

        // Both should parse to same structure
        assert_eq!(nodes1.len(), nodes2.len());
        assert!(matches!(&nodes1[0], Node::HStack { .. }));
        assert!(matches!(&nodes2[0], Node::HStack { .. }));
    }

    #[test]
    fn test_parse_attributes() {
        let md = r#"<vstack flex="0.5">
<input name="test" placeholder="Type here" />
</vstack>"#;
        let nodes = parse_body(md).unwrap();

        match &nodes[0] {
            Node::VStack { flex, children, .. } => {
                assert_eq!(*flex, Some(0.5));
                assert_eq!(children.len(), 1);

                match &children[0] {
                    Node::Input { name, placeholder } => {
                        assert_eq!(name, "test");
                        assert_eq!(placeholder.as_deref(), Some("Type here"));
                    }
                    _ => panic!("Expected Input"),
                }
            }
            _ => panic!("Expected VStack"),
        }
    }

    #[test]
    fn test_parse_expression_attributes() {
        let md = r#"<each from={queries.feed} as="note">
{note.content}
</each>"#;
        let nodes = parse_body(md).unwrap();

        match &nodes[0] {
            Node::Each { from, as_name, children } => {
                assert_eq!(from, "queries.feed");
                assert_eq!(as_name, "note");
                assert_eq!(children.len(), 1);
            }
            _ => panic!("Expected Each"),
        }
    }

    #[test]
    fn test_parse_markdown_with_components() {
        let md = r#"# Title

Some **bold** text.

<vstack>
<button label="Click" />
</vstack>

More text."#;

        let nodes = parse_body(md).unwrap();

        assert!(nodes.len() >= 4);
        assert!(matches!(&nodes[0], Node::Heading { .. }));
        assert!(matches!(&nodes[1], Node::Paragraph { .. }));
        assert!(matches!(&nodes[2], Node::VStack { .. }));
    }

    #[test]
    fn test_parse_inline_expressions() {
        let md = r#"# {state.title}

Hello {user.name}!"#;

        let nodes = parse_body(md).unwrap();

        // First should be heading with expression
        match &nodes[0] {
            Node::Heading { children, .. } => {
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], Node::Expr { expression } if expression == "state.title"));
            }
            _ => panic!("Expected heading"),
        }

        // Second should be paragraph with text and expression
        match &nodes[1] {
            Node::Paragraph { children } => {
                assert!(children.len() >= 2);
                // Should have both text and expr nodes
                let has_text = children.iter().any(|c| matches!(c, Node::Text { .. }));
                let has_expr = children.iter().any(|c| matches!(c, Node::Expr { expression } if expression == "user.name"));
                assert!(has_text && has_expr);
            }
            _ => panic!("Expected paragraph"),
        }
    }
}
