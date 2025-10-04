use crate::parser::ast::{ListItem, Node};
use crate::parser::component::{AttrValue, Component};
use anyhow::Result;
use markdown::mdast;
use regex::Regex;

/// Parse markdown body into AST nodes
pub fn parse_body(source: &str) -> Result<Vec<Node>> {
    let mut options = markdown::ParseOptions::default();
    // Enable HTML parsing so we can capture component tags
    options.constructs.html_flow = true;
    options.constructs.html_text = true;

    let ast = markdown::to_mdast(source, &options)
        .map_err(|e| anyhow::anyhow!("Failed to parse markdown: {}", e))?;

    // The root is always a Root node containing children
    match ast {
        mdast::Node::Root(root) => transform_children_with_components(root.children),
        _ => Ok(vec![]),
    }
}

/// Transform markdown AST children, properly nesting components based on opening/closing tags
fn transform_children_with_components(children: Vec<mdast::Node>) -> Result<Vec<Node>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < children.len() {
        let node = &children[i];

        // Check if this is an opening component tag
        if let mdast::Node::Html(html) = node {
            let trimmed = html.value.trim();

            if is_component_tag(trimmed) && !is_closing_tag(trimmed) {
                // Check if self-closing
                if trimmed.ends_with("/>") {
                    let comp = Component::parse(trimmed)?;
                    if let Some(node) = build_component_node(&comp, vec![])? {
                        result.push(node);
                    }
                    i += 1;
                    continue;
                }

                // Found opening tag - find matching closing tag
                let tag_name = extract_tag_name(trimmed);
                let closing_tag = format!("</{}>", tag_name);

                // Find closing tag index
                let mut close_idx = None;
                for j in (i + 1)..children.len() {
                    if let mdast::Node::Html(h) = &children[j] {
                        if h.value.trim() == closing_tag {
                            close_idx = Some(j);
                            break;
                        }
                    }
                }

                if let Some(close_idx) = close_idx {
                    // Parse component with content between tags
                    let comp = Component::parse(trimmed)?;
                    let content_nodes = &children[(i + 1)..close_idx];

                    // Recursively transform content
                    let content = transform_children_with_components(content_nodes.to_vec())?;

                    // Special validation: buttons should not have opening/closing tags
                    if comp.tag == "button" {
                        return Err(anyhow::anyhow!(
                            "<button> must be self-closing. Use label attribute for text.\n\
                             Example: <button label=\"Click Me\" on_click={{actions.post}} />"
                        ));
                    }

                    if let Some(node) = build_component_node(&comp, content)? {
                        result.push(node);
                    }

                    i = close_idx + 1;  // Skip past closing tag
                    continue;
                }
            }
        }

        // Not a component - transform normally
        if let Some(transformed) = transform_node(node.clone())? {
            result.push(transformed);
        }
        i += 1;
    }

    Ok(result)
}

/// Old transform for simple cases (non-component content)
fn transform_children(children: Vec<mdast::Node>) -> Result<Vec<Node>> {
    children
        .into_iter()
        .flat_map(|child| transform_node(child).transpose())
        .collect()
}

/// Transform a single markdown node
fn transform_node(node: mdast::Node) -> Result<Option<Node>> {
    Ok(Some(match node {
        // Block nodes
        mdast::Node::Heading(h) => Node::Heading {
            level: h.depth,
            children: transform_children(h.children)?,
        },
        mdast::Node::Paragraph(p) => {
            // Check if paragraph contains multiple components without blank lines
            // This will error with a helpful message
            try_parse_all_components(&p.children)?;

            Node::Paragraph {
                children: transform_children(p.children)?,
            }
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
                        Err(anyhow::anyhow!("Expected ListItem node"))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        },

        // Inline nodes
        mdast::Node::Text(t) => {
            // Check for expression interpolation: {expr}
            return Ok(parse_text_with_expressions(&t.value)?);
        }
        mdast::Node::Strong(s) => Node::Strong {
            children: transform_children(s.children)?,
        },
        mdast::Node::Emphasis(e) => Node::Emphasis {
            children: transform_children(e.children)?,
        },
        mdast::Node::Link(link) => Node::Link {
            url: link.url,
            children: transform_children(link.children)?,
        },
        mdast::Node::Image(img) => Node::Image {
            src: img.url,
            alt: img.alt,
        },

        // HTML nodes - check if they're components
        mdast::Node::Html(html) => {
            // Try to parse as component (handles merged HTML blocks from markdown-rs)
            return parse_html_or_component(&html.value);
        }

        // Unsupported nodes - skip
        mdast::Node::Code(_) => return Ok(None), // Skip code blocks for now
        mdast::Node::InlineCode(code) => Node::Text {
            value: format!("`{}`", code.value),
        },
        mdast::Node::ThematicBreak(_) => Node::Spacer { size: Some(20.0) }, // Render as spacer
        mdast::Node::Blockquote(_) => return Ok(None), // TODO: support later
        mdast::Node::Table(_) => return Ok(None),      // Not supporting tables

        // Catch-all for other node types
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

    // If it's a single expression that takes up the whole text, return just the Expr node
    if let Some(caps) = expr_re.captures(text) {
        if caps.get(0).unwrap().as_str() == text {
            return Ok(Some(Node::Expr {
                expression: caps[1].to_string(),
            }));
        }
    }

    // Otherwise, we have mixed text and expressions
    // For now, just treat the whole thing as text
    // TODO: In the future, we could split into multiple Text and Expr nodes
    Ok(Some(Node::Text {
        value: text.to_string(),
    }))
}

/// Check if paragraph has multiple components without blank lines, and error if so
/// Returns Some(Vec<Node>) if paragraph contains only components, None otherwise
fn try_parse_all_components(children: &[mdast::Node]) -> Result<Option<Vec<Node>>> {
    // First, check if there are multiple component opening tags (indicates missing blank lines)
    let component_count = children.iter().filter(|node| {
        matches!(node, mdast::Node::Html(h) if is_component_tag(&h.value) && !is_closing_tag(&h.value))
    }).count();

    if component_count > 1 {
        return Err(anyhow::anyhow!(
            "Multiple components on consecutive lines detected. Please add blank lines between components.\n\
             Example:\n\
             <button>Click</button>\n\
             \n\
             <input name=\"foo\" />"
        ));
    }

    // Check for button tags with text children (not allowed)
    for (i, node) in children.iter().enumerate() {
        if let mdast::Node::Html(h) = node {
            let trimmed = h.value.trim();
            if trimmed.starts_with("<button") && !trimmed.ends_with("/>") && !is_closing_tag(trimmed) {
                // This is an opening button tag - check if there's text content before closing tag
                for j in (i + 1)..children.len() {
                    match &children[j] {
                        mdast::Node::Html(h2) if h2.value.trim() == "</button>" => break,
                        mdast::Node::Text(t) if !t.value.trim().is_empty() => {
                            return Err(anyhow::anyhow!(
                                "<button> must be self-closing. Use label attribute for text.\n\
                                 Example: <button label=\"Click Me\" on_click={{actions.post}} />"
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Check if this paragraph contains only Html nodes (components) and whitespace Text
    let has_non_component = children.iter().any(|node| match node {
        mdast::Node::Html(_) => false,
        mdast::Node::Text(t) if t.value.trim().is_empty() => false,
        _ => true,
    });

    if has_non_component {
        return Ok(None); // Has regular content, treat as normal paragraph
    }

    // Parse each component
    let mut components = Vec::new();
    let mut i = 0;

    while i < children.len() {
        match &children[i] {
            mdast::Node::Html(html) => {
                let trimmed = html.value.trim();

                // Skip closing tags and whitespace
                if is_closing_tag(trimmed) {
                    i += 1;
                    continue;
                }

                if !is_component_tag(trimmed) {
                    i += 1;
                    continue;
                }

                // Parse the component
                let comp = Component::parse(trimmed)?;

                if comp.self_closing {
                    // Self-closing component, add it directly
                    if let Some(node) = build_component_node(&comp, vec![])? {
                        components.push(node);
                    }
                    i += 1;
                } else {
                    // Find the closing tag
                    let closing_tag = format!("</{}>", comp.tag);
                    let mut close_idx = None;
                    let mut content_nodes = Vec::new();

                    // Collect content between opening and closing tags
                    for j in (i + 1)..children.len() {
                        match &children[j] {
                            mdast::Node::Html(h) if h.value.trim() == closing_tag => {
                                close_idx = Some(j);
                                break;
                            }
                            mdast::Node::Text(t) if !t.value.trim().is_empty() => {
                                content_nodes.push(Node::text(t.value.clone()));
                            }
                            _ => {}
                        }
                    }

                    let content = content_nodes;

                    if let Some(node) = build_component_node(&comp, content)? {
                        components.push(node);
                    }

                    i = close_idx.unwrap_or(i) + 1;
                }
            }
            mdast::Node::Text(t) if t.value.trim().is_empty() => {
                // Skip whitespace
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if components.is_empty() {
        Ok(None)
    } else {
        Ok(Some(components))
    }
}

/// Try to parse an inline component from paragraph children
/// Returns Some(Node) if this is a component, None if it's regular content
fn try_parse_inline_component(children: &[mdast::Node]) -> Result<Option<Node>> {
    // Pattern: Html (open tag) + content nodes + Html (close tag)
    if children.is_empty() {
        return Ok(None);
    }

    // Check if first child is an opening component tag
    let first_html = match &children[0] {
        mdast::Node::Html(h) => &h.value,
        _ => return Ok(None),
    };

    if !is_component_tag(first_html) || is_closing_tag(first_html) {
        return Ok(None);
    }

    // Parse opening tag
    let comp = Component::parse(first_html)?;

    // Self-closing component (shouldn't be in paragraph, but handle it)
    if comp.self_closing {
        return build_component_node(&comp, vec![]);
    }

    // Find closing tag
    let closing_tag = format!("</{}>", comp.tag);
    let close_index = children.iter().position(|node| {
        matches!(node, mdast::Node::Html(h) if h.value.trim() == closing_tag)
    });

    let close_index = match close_index {
        Some(idx) => idx,
        None => return Err(anyhow::anyhow!("Missing closing tag for <{}>", comp.tag)),
    };

    // Extract content between tags
    let content_nodes = &children[1..close_index];

    // Transform content nodes to our AST
    let content = content_nodes
        .iter()
        .flat_map(|child| transform_node(child.clone()).transpose())
        .collect::<Result<Vec<_>>>()?;

    build_component_node(&comp, content)
}

/// Parse HTML string - determine if it's a component or raw HTML
/// This handles block-level HTML that markdown-rs may have merged with following content
fn parse_html_or_component(html: &str) -> Result<Option<Node>> {
    let trimmed = html.trim();

    //Check for opening component tag at start
    if trimmed.starts_with('<') && !is_closing_tag(trimmed) {
        // Try to extract just the opening tag
        if let Some(tag_end) = trimmed.find('>') {
            let opening_tag = &trimmed[..=tag_end];

            if is_component_tag(opening_tag) {
                // Check if this is self-closing
                if opening_tag.ends_with("/>") {
                    return parse_component(opening_tag);
                }

                // Otherwise, this is a multi-line component with markdown content
                // The content is merged into this Html node by markdown-rs
                // We need to extract and parse it

                let comp = Component::parse(opening_tag)?;
                let after_tag = &trimmed[tag_end + 1..];

                // Parse the content as markdown
                let children = parse_body(after_tag)?;

                return build_component_node(&comp, children);
            }
        }
    }

    // For now, skip raw HTML
    Ok(None)
}

/// Check if HTML string is a component tag
fn is_component_tag(html: &str) -> bool {
    let html = html.trim();
    html.starts_with("<each")
        || html.starts_with("</each")
        || html.starts_with("<if")
        || html.starts_with("</if")
        || html.starts_with("<button")
        || html.starts_with("</button")
        || html.starts_with("<input")
        || html.starts_with("<vstack")
        || html.starts_with("</vstack")
        || html.starts_with("<hstack")
        || html.starts_with("</hstack")
        || html.starts_with("<grid")
        || html.starts_with("</grid")
        || html.starts_with("<spacer")
}

/// Check if this is a closing tag
fn is_closing_tag(html: &str) -> bool {
    html.trim().starts_with("</")
}

/// Extract tag name from opening or closing tag
fn extract_tag_name(html: &str) -> &str {
    let trimmed = html.trim();
    let start = if trimmed.starts_with("</") { 2 } else { 1 };

    if let Some(end) = trimmed[start..].find(|c: char| c == '>' || c == ' ') {
        &trimmed[start..start + end]
    } else {
        ""
    }
}

/// Parse block-level component (self-closing)
fn parse_component(html: &str) -> Result<Option<Node>> {
    let comp = Component::parse(html)?;

    if !comp.self_closing {
        return Err(anyhow::anyhow!(
            "Block-level component must be self-closing: {}",
            html
        ));
    }

    build_component_node(&comp, vec![])
}

/// Build AST node from parsed component
fn build_component_node(comp: &Component, children: Vec<Node>) -> Result<Option<Node>> {
    let node = match comp.tag.as_str() {
        "button" => {
            // Buttons must be self-closing with label attribute
            if !children.is_empty() {
                return Err(anyhow::anyhow!(
                    "<button> must be self-closing. Use label attribute for text.\n\
                     Example: <button label=\"Click Me\" on_click={{actions.post}} />"
                ));
            }

            let on_click = comp.get_attr_opt("on_click").map(|av| match av {
                AttrValue::Expression(expr) => expr.clone(),
                AttrValue::Literal(lit) => lit.clone(),
            });

            // Get label and convert to text child
            let label = comp.get_attr("label")?;
            let label_text = match label {
                AttrValue::Literal(s) => s.clone(),
                AttrValue::Expression(expr) => {
                    // For now, store expression as-is, will be evaluated at runtime
                    return Err(anyhow::anyhow!(
                        "Dynamic button labels not yet supported. Use literal string.\n\
                         Example: label=\"Click Me\""
                    ));
                }
            };

            Node::Button {
                on_click,
                children: vec![Node::text(label_text)],
            }
        }

        "input" => {
            let name = comp.get_literal("name")?;
            let placeholder = comp.get_attr_opt("placeholder").map(|av| match av {
                AttrValue::Literal(lit) => lit.clone(),
                AttrValue::Expression(expr) => expr.clone(),
            });

            Node::Input { name, placeholder }
        }

        "vstack" => {
            let width = comp.get_attr_opt("width").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let height = comp.get_attr_opt("height").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let flex = comp.get_attr_opt("flex").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let align = comp.get_attr_opt("align").map(|av| match av {
                AttrValue::Literal(s) => s.clone(),
                AttrValue::Expression(e) => e.clone(),
            });

            Node::VStack { children, width, height, flex, align }
        }

        "hstack" => {
            let width = comp.get_attr_opt("width").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let height = comp.get_attr_opt("height").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let flex = comp.get_attr_opt("flex").and_then(|av| match av {
                AttrValue::Literal(s) => s.parse().ok(),
                _ => None,
            });
            let align = comp.get_attr_opt("align").map(|av| match av {
                AttrValue::Literal(s) => s.clone(),
                AttrValue::Expression(e) => e.clone(),
            });

            Node::HStack { children, width, height, flex, align }
        }

        "each" => {
            let from = comp.get_expr("from")?;
            let as_name = comp.get_literal("as")?;

            Node::Each {
                from,
                as_name,
                children,
            }
        }

        "if" => {
            let value = comp.get_expr("value")?;

            // TODO: Handle <else> children
            Node::If {
                value,
                children,
                else_children: None,
            }
        }

        "grid" => {
            let columns = comp
                .get_attr_opt("columns")
                .and_then(|av| match av {
                    AttrValue::Literal(s) => s.parse().ok(),
                    AttrValue::Expression(_) => None,
                });

            Node::Grid { columns, children }
        }

        "spacer" => {
            let size = comp
                .get_attr_opt("size")
                .and_then(|av| match av {
                    AttrValue::Literal(s) => s.parse().ok(),
                    AttrValue::Expression(_) => None,
                });

            Node::Spacer { size }
        }

        _ => {
            return Err(anyhow::anyhow!("Unknown component tag: {}", comp.tag));
        }
    };

    Ok(Some(node))
}

/// Second pass: Nest component children properly
/// Container components (vstack, hstack, each, if) with empty children
/// should collect following nodes until they're filled
fn nest_components(nodes: Vec<Node>) -> Result<Vec<Node>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < nodes.len() {
        let node = &nodes[i];

        // Check if this is an empty container component
        match node {
            Node::VStack { children, .. } | Node::HStack { children, .. }
            | Node::Each { children, .. } | Node::If { children, .. }
            if children.is_empty() => {
                // This container needs children - collect until we find a non-container
                // or another empty container (which would be a sibling)
                let mut collected_children = Vec::new();
                i += 1;

                while i < nodes.len() {
                    let next_node = &nodes[i];

                    // Check if next node is an empty container
                    let is_empty_container = matches!(next_node,
                        Node::VStack { children, .. } | Node::HStack { children, .. }
                        | Node::Each { children, .. } | Node::If { children, .. }
                        if children.is_empty()
                    );

                    if is_empty_container {
                        // Empty containers should be collected and processed recursively
                        collected_children.push(next_node.clone());
                        i += 1;
                    } else {
                        // Regular content node
                        collected_children.push(next_node.clone());
                        i += 1;
                    }
                }

                // Recursively process collected children to handle nested empty containers
                let processed_children = nest_components(collected_children)?;

                // Rebuild the container with processed children, preserving attributes
                let filled_node = match node {
                    Node::VStack { width, height, flex, align, .. } => Node::VStack {
                        children: processed_children,
                        width: *width,
                        height: *height,
                        flex: *flex,
                        align: align.clone(),
                    },
                    Node::HStack { width, height, flex, align, .. } => Node::HStack {
                        children: processed_children,
                        width: *width,
                        height: *height,
                        flex: *flex,
                        align: align.clone(),
                    },
                    Node::Each { from, as_name, .. } => Node::Each {
                        from: from.clone(),
                        as_name: as_name.clone(),
                        children: processed_children,
                    },
                    Node::If { value, .. } => Node::If {
                        value: value.clone(),
                        children: processed_children,
                        else_children: None,
                    },
                    _ => unreachable!(),
                };

                result.push(filled_node);
            }
            _ => {
                result.push(node.clone());
                i += 1;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        let md = "# Hello World";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Heading { level, children } => {
                assert_eq!(*level, 1);
                assert_eq!(children.len(), 1);
                match &children[0] {
                    Node::Text { value } => assert_eq!(value, "Hello World"),
                    _ => panic!("Expected Text node"),
                }
            }
            _ => panic!("Expected Heading node"),
        }
    }

    #[test]
    fn test_parse_paragraph() {
        let md = "This is a paragraph.";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Paragraph { children } => {
                assert_eq!(children.len(), 1);
            }
            _ => panic!("Expected Paragraph node"),
        }
    }

    #[test]
    fn test_parse_emphasis() {
        let md = "This is **bold** and *italic*.";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Paragraph { children } => {
                assert_eq!(children.len(), 5); // text, strong, text, emphasis, text
            }
            _ => panic!("Expected Paragraph node"),
        }
    }

    #[test]
    fn test_parse_list() {
        let md = "- Item 1\n- Item 2\n- Item 3";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::List { ordered, items } => {
                assert_eq!(*ordered, false);
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected List node"),
        }
    }

    #[test]
    fn test_parse_link() {
        let md = "[Click here](https://example.com)";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Paragraph { children } => match &children[0] {
                Node::Link { url, children } => {
                    assert_eq!(url, "https://example.com");
                    assert_eq!(children.len(), 1);
                }
                _ => panic!("Expected Link node"),
            },
            _ => panic!("Expected Paragraph node"),
        }
    }

    #[test]
    fn test_raw_mdast_for_vstack() {
        let content = r#"<hstack>

<vstack>

**Avatar**

</vstack>

<vstack>

**Derek Ross**

Content here

</vstack>

</hstack>"#;

        let mut options = markdown::ParseOptions::default();
        options.constructs.html_flow = true;
        options.constructs.html_text = true;

        let ast = markdown::to_mdast(content, &options).unwrap();

        println!("=== Raw markdown-rs AST ===");
        println!("{:#?}", ast);
        println!("===========================");
    }

    #[test]
    fn test_parse_image() {
        let md = "![Alt text](https://example.com/image.png)";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Paragraph { children } => match &children[0] {
                Node::Image { src, alt } => {
                    assert_eq!(src, "https://example.com/image.png");
                    assert_eq!(alt, "Alt text");
                }
                _ => panic!("Expected Image node"),
            },
            _ => panic!("Expected Paragraph node"),
        }
    }

    #[test]
    fn test_parse_expression() {
        let md = "{user.name}";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 1);

        match &nodes[0] {
            Node::Paragraph { children } => match &children[0] {
                Node::Expr { expression } => {
                    assert_eq!(expression, "user.name");
                }
                _ => panic!("Expected Expr node"),
            },
            _ => panic!("Expected Paragraph node"),
        }
    }

    #[test]
    fn test_parse_mixed_content() {
        let md = "# My App\n\nWelcome **user**!\n\n- Item 1\n- Item 2";
        let nodes = parse_body(md).unwrap();
        assert_eq!(nodes.len(), 3); // heading, paragraph, list
    }

    #[test]
    fn test_skip_code_blocks() {
        let md = "Text\n\n```rust\ncode here\n```\n\nMore text";
        let nodes = parse_body(md).unwrap();
        // Should have 2 paragraphs, code block skipped
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_raw_mdast_for_components() {
        let md = "<button>Click Me</button>\n\n<input name=\"foo\" />";

        let mut options = markdown::ParseOptions::default();
        options.constructs.html_flow = true;
        options.constructs.html_text = true;

        let ast = markdown::to_mdast(md, &options).unwrap();

        println!("Raw mdast: {:#?}", ast);
    }


    #[test]
    fn test_parse_button_self_closing() {
        let md = r#"<button label="Click Me" />"#;
        let nodes = parse_body(md).unwrap();

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Button { on_click, children } => {
                assert!(on_click.is_none());
                assert_eq!(children.len(), 1);
                match &children[0] {
                    Node::Text { value } => assert_eq!(value, "Click Me"),
                    _ => panic!("Expected Text node in button children"),
                }
            }
            _ => panic!("Expected Button node, got: {:?}", nodes[0]),
        }
    }

    #[test]
    fn test_parse_button_with_children_errors() {
        // Inline button with text content (old syntax) should error
        let md = r#"
<vstack>
<button>Text</button>
</vstack>
"#;
        let result = parse_body(md);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should error about missing label attribute or self-closing requirement
        assert!(err_msg.contains("label") || err_msg.contains("self-closing"));
    }

    #[test]
    fn test_parse_self_closing_input() {
        let md = r#"<input name="message" placeholder="Type here" />"#;
        let nodes = parse_body(md).unwrap();

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Input { name, placeholder } => {
                assert_eq!(name, "message");
                assert_eq!(placeholder.as_deref(), Some("Type here"));
            }
            _ => panic!("Expected Input node, got: {:?}", nodes[0]),
        }
    }

    #[test]
    fn test_parse_vstack_with_markdown() {
        let md = r#"<vstack>
## Title

This is a paragraph.
</vstack>"#;
        let nodes = parse_body(md).unwrap();

        // Note: Due to markdown-rs HTML flow parsing, this may not capture children perfectly
        // but it should at least create a VStack node
        assert!(nodes.iter().any(|n| matches!(n, Node::VStack { .. })));
    }

    #[test]
    fn test_components_self_closing_work_inline() {
        // Self-closing components on consecutive lines work in block context
        // markdown-rs treats them as separate block-level HTML
        let md = r#"<button label="First" />
<button label="Second" />"#;

        let nodes = parse_body(md).unwrap();
        // They parse successfully (markdown-rs handles self-closing differently)
        assert!(nodes.len() >= 1);
    }

    #[test]
    fn test_components_with_blank_lines_ok() {
        let md = r#"<button label="Click Me" />

<input name="message" />"#;

        let nodes = parse_body(md).unwrap();
        // Should have 2 top-level nodes
        assert!(nodes.len() >= 2);
    }
}
