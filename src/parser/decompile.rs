use crate::parser::ast::{Action, Document, Filter, Frontmatter, ListItem, Node, Pipe};

/// Decompile a Document AST back to .hnmd format
pub fn decompile(doc: &Document) -> String {
    let mut output = String::new();

    // Write frontmatter if not empty
    if !doc.frontmatter.filters.is_empty()
        || !doc.frontmatter.pipes.is_empty()
        || !doc.frontmatter.actions.is_empty()
        || !doc.frontmatter.state.is_empty()
    {
        output.push_str("---\n");
        output.push_str(&decompile_frontmatter(&doc.frontmatter));
        output.push_str("---\n\n");
    }

    // Write body
    for node in &doc.body {
        output.push_str(&decompile_node(node, 0));
    }

    output
}

/// Decompile frontmatter to YAML
fn decompile_frontmatter(fm: &Frontmatter) -> String {
    let mut output = String::new();

    // Filters section
    if !fm.filters.is_empty() {
        output.push_str("filters:\n");
        let mut filter_ids: Vec<_> = fm.filters.keys().collect();
        filter_ids.sort();
        for id in filter_ids {
            let filter = &fm.filters[id];
            output.push_str(&format!("  {}:\n", id));
            output.push_str(&decompile_filter(filter, 4));
        }
        output.push('\n');
    }

    // Pipes section
    if !fm.pipes.is_empty() {
        output.push_str("pipes:\n");
        let mut pipe_ids: Vec<_> = fm.pipes.keys().collect();
        pipe_ids.sort();
        for id in pipe_ids {
            let pipe = &fm.pipes[id];
            output.push_str(&format!("  {}:\n", id));
            output.push_str(&decompile_pipe(pipe, 4));
        }
        output.push('\n');
    }

    // Actions section
    if !fm.actions.is_empty() {
        output.push_str("actions:\n");
        let mut action_ids: Vec<_> = fm.actions.keys().collect();
        action_ids.sort();
        for id in action_ids {
            let action = &fm.actions[id];
            output.push_str(&format!("  {}:\n", id));
            output.push_str(&decompile_action(action, 4));
        }
        output.push('\n');
    }

    // State section
    if !fm.state.is_empty() {
        output.push_str("state:\n");
        let mut state_keys: Vec<_> = fm.state.keys().collect();
        state_keys.sort();
        for key in state_keys {
            let value = &fm.state[key];
            output.push_str(&format!("  {}: {}\n", key, decompile_json_value(value)));
        }
        output.push('\n');
    }

    output
}

/// Decompile a filter to YAML
fn decompile_filter(filter: &Filter, indent: usize) -> String {
    let mut output = String::new();
    let indent_str = " ".repeat(indent);

    if let Some(kinds) = &filter.kinds {
        output.push_str(&format!("{}kinds: {:?}\n", indent_str, kinds));
    }

    if let Some(authors) = &filter.authors {
        output.push_str(&format!("{}authors: {:?}\n", indent_str, authors));
    }

    if let Some(ids) = &filter.ids {
        output.push_str(&format!("{}ids: {:?}\n", indent_str, ids));
    }

    if let Some(e_tags) = &filter.e_tags {
        output.push_str(&format!("{}\"#e\": {:?}\n", indent_str, e_tags));
    }

    if let Some(p_tags) = &filter.p_tags {
        output.push_str(&format!("{}\"#p\": {:?}\n", indent_str, p_tags));
    }

    if let Some(since) = filter.since {
        output.push_str(&format!("{}since: {}\n", indent_str, since));
    }

    if let Some(until) = filter.until {
        output.push_str(&format!("{}until: {}\n", indent_str, until));
    }

    if let Some(limit) = filter.limit {
        output.push_str(&format!("{}limit: {}\n", indent_str, limit));
    }

    output
}

/// Decompile a pipe to YAML
fn decompile_pipe(pipe: &Pipe, indent: usize) -> String {
    let indent_str = " ".repeat(indent);
    format!("{}from: {}\n{}jq: \"{}\"\n", indent_str, pipe.from, indent_str, pipe.jq)
}

/// Decompile an action to YAML
fn decompile_action(action: &Action, indent: usize) -> String {
    let mut output = String::new();
    let indent_str = " ".repeat(indent);

    output.push_str(&format!("{}kind: {}\n", indent_str, action.kind));
    output.push_str(&format!("{}content: \"{}\"\n", indent_str, action.content));

    if !action.tags.is_empty() {
        output.push_str(&format!("{}tags:\n", indent_str));
        for tag in &action.tags {
            output.push_str(&format!("{}  - {:?}\n", indent_str, tag));
        }
    }

    output
}

/// Decompile JSON value for state
fn decompile_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("\"{}\"", s),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(decompile_json_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(_) => serde_json::to_string(value).unwrap(),
    }
}

/// Decompile a node to markdown
fn decompile_node(node: &Node, indent: usize) -> String {
    match node {
        Node::Heading { level, children } => {
            format!("{} {}\n\n", "#".repeat(*level as usize), decompile_children(children, indent))
        }
        Node::Paragraph { children } => {
            format!("{}\n\n", decompile_children(children, indent))
        }
        Node::Text { value } => value.clone(),
        Node::Strong { children } => {
            format!("**{}**", decompile_children(children, indent))
        }
        Node::Emphasis { children } => {
            format!("*{}*", decompile_children(children, indent))
        }
        Node::List { ordered, items } => {
            let mut output = String::new();
            for (i, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}. ", i + 1)
                } else {
                    "- ".to_string()
                };
                output.push_str(&marker);
                output.push_str(&decompile_list_item(item, indent));
                output.push('\n');
            }
            output.push('\n');
            output
        }
        Node::Link { url, children } => {
            format!("[{}]({})", decompile_children(children, indent), url)
        }
        Node::Image { src, alt } => {
            format!("![{}]({})", alt, src)
        }
        Node::Expr { expression } => {
            format!("{{{}}}", expression)
        }
        Node::Each { from, as_name, children } => {
            let mut output = String::new();
            output.push_str(&format!("<each from={{{}}} as=\"{}\">\n", from, as_name));
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            output.push_str("</each>\n\n");
            output
        }
        Node::If { value, children, else_children } => {
            let mut output = String::new();
            output.push_str(&format!("<if value={{{}}}>\n", value));
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            if let Some(else_children) = else_children {
                output.push_str("<else>\n");
                for child in else_children {
                    output.push_str(&decompile_node(child, indent + 2));
                }
            }
            output.push_str("</if>\n\n");
            output
        }
        Node::Button { on_click, children } => {
            let mut output = String::new();
            if let Some(action) = on_click {
                output.push_str(&format!("<button on_click={{{}}}>\n", action));
            } else {
                output.push_str("<button>\n");
            }
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            output.push_str("</button>\n\n");
            output
        }
        Node::Input { name, placeholder } => {
            if let Some(ph) = placeholder {
                format!("<input name=\"{}\" placeholder=\"{}\" />\n\n", name, ph)
            } else {
                format!("<input name=\"{}\" />\n\n", name)
            }
        }
        Node::VStack { children, width, height, flex, align } => {
            let mut output = String::new();
            output.push_str("<vstack");
            if let Some(w) = width { output.push_str(&format!(" width=\"{}\"", w)); }
            if let Some(h) = height { output.push_str(&format!(" height=\"{}\"", h)); }
            if let Some(f) = flex { output.push_str(&format!(" flex=\"{}\"", f)); }
            if let Some(a) = align { output.push_str(&format!(" align=\"{}\"", a)); }
            output.push_str(">\n");
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            output.push_str("</vstack>\n\n");
            output
        }
        Node::HStack { children, width, height, flex, align } => {
            let mut output = String::new();
            output.push_str("<hstack");
            if let Some(w) = width { output.push_str(&format!(" width=\"{}\"", w)); }
            if let Some(h) = height { output.push_str(&format!(" height=\"{}\"", h)); }
            if let Some(f) = flex { output.push_str(&format!(" flex=\"{}\"", f)); }
            if let Some(a) = align { output.push_str(&format!(" align=\"{}\"", a)); }
            output.push_str(">\n");
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            output.push_str("</hstack>\n\n");
            output
        }
        Node::Grid { columns, children } => {
            let mut output = String::new();
            if let Some(cols) = columns {
                output.push_str(&format!("<grid columns={{{}}}>\n", cols));
            } else {
                output.push_str("<grid>\n");
            }
            for child in children {
                output.push_str(&decompile_node(child, indent + 2));
            }
            output.push_str("</grid>\n\n");
            output
        }
        Node::Spacer { size } => {
            if let Some(s) = size {
                format!("<spacer size={{{}}} />\n\n", s)
            } else {
                "<spacer />\n\n".to_string()
            }
        }
    }
}

/// Decompile children nodes
fn decompile_children(children: &[Node], indent: usize) -> String {
    children
        .iter()
        .map(|child| decompile_node(child, indent))
        .collect::<Vec<_>>()
        .join("")
        .trim_end()
        .to_string()
}

/// Decompile list item
fn decompile_list_item(item: &ListItem, indent: usize) -> String {
    decompile_children(&item.children, indent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::*;

    #[test]
    fn test_decompile_empty_document() {
        let doc = Document::new(Frontmatter::new(), vec![]);
        let output = decompile(&doc);
        assert_eq!(output, "");
    }

    #[test]
    fn test_decompile_simple_markdown() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![
                Node::heading(1, vec![Node::text("Hello")]),
                Node::paragraph(vec![Node::text("World")]),
            ],
        );
        let output = decompile(&doc);
        assert_eq!(output, "# Hello\n\nWorld\n\n");
    }

    #[test]
    fn test_decompile_frontmatter() {
        let doc = Document::new(
            Frontmatter::new()
                .with_filter("feed", Filter::new().kinds(vec![1]).limit(20))
                .with_pipe("feed_content", Pipe::new("feed", "map(.content)"))
                .with_action("post", Action::new(1, "Hello")),
            vec![],
        );
        let output = decompile(&doc);
        assert!(output.contains("filters:"));
        assert!(output.contains("feed:"));
        assert!(output.contains("kinds: [1]"));
        assert!(output.contains("limit: 20"));
        assert!(output.contains("pipes:"));
        assert!(output.contains("feed_content:"));
        assert!(output.contains("actions:"));
        assert!(output.contains("post:"));
    }

    #[test]
    fn test_decompile_expression() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![Node::paragraph(vec![Node::expr("user.name")])],
        );
        let output = decompile(&doc);
        assert_eq!(output, "{user.name}\n\n");
    }

    #[test]
    fn test_decompile_component() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![Node::each(
                "queries.feed",
                "item",
                vec![Node::paragraph(vec![Node::expr("item.content")])],
            )],
        );
        let output = decompile(&doc);
        assert!(output.contains("<each from={queries.feed} as=\"item\">"));
        assert!(output.contains("{item.content}"));
        assert!(output.contains("</each>"));
    }

    #[test]
    fn test_decompile_button() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![Node::button(
                Some("actions.post".to_string()),
                vec![Node::text("Click me")],
            )],
        );
        let output = decompile(&doc);
        assert!(output.contains("<button on_click={actions.post}>"));
        assert!(output.contains("Click me"));
        assert!(output.contains("</button>"));
    }

    #[test]
    fn test_decompile_input() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![Node::input("message")],
        );
        let output = decompile(&doc);
        assert!(output.contains("<input name=\"message\" />"));
    }

    #[test]
    fn test_decompile_list() {
        let doc = Document::new(
            Frontmatter::new(),
            vec![Node::List {
                ordered: false,
                items: vec![
                    ListItem {
                        children: vec![Node::text("Item 1")],
                    },
                    ListItem {
                        children: vec![Node::text("Item 2")],
                    },
                ],
            }],
        );
        let output = decompile(&doc);
        assert!(output.contains("- Item 1"));
        assert!(output.contains("- Item 2"));
    }

    #[test]
    fn test_roundtrip_simple() {
        use crate::parser::frontmatter::parse_frontmatter;
        use crate::parser::mdx::parse_body;

        // Create a document
        let original = Document::new(
            Frontmatter::new()
                .with_filter("feed", Filter::new().kinds(vec![1]).limit(10)),
            vec![
                Node::heading(1, vec![Node::text("Test")]),
                Node::paragraph(vec![Node::text("Content")]),
            ],
        );

        // Decompile to string
        let hnmd = decompile(&original);

        // Parse back
        let parts: Vec<&str> = hnmd.split("---").collect();
        assert_eq!(parts.len(), 3); // empty, frontmatter, body

        let fm = parse_frontmatter(parts[1]).unwrap();
        let body = parse_body(parts[2]).unwrap();

        // Verify frontmatter
        assert_eq!(fm.filters.len(), 1);
        assert!(fm.filters.contains_key("feed"));

        // Verify body
        assert_eq!(body.len(), 2);
    }
}
