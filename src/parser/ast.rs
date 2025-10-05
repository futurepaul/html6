use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level HNMD document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Version string (e.g., "1.0.0")
    pub version: String,
    /// Frontmatter containing filters, pipes, actions, and state
    pub frontmatter: Frontmatter,
    /// Markdown body rendered as nodes
    pub body: Vec<Node>,
}

impl Document {
    pub fn new(frontmatter: Frontmatter, body: Vec<Node>) -> Self {
        Self {
            version: "1.0.0".to_string(),
            frontmatter,
            body,
        }
    }
}

/// HNMD frontmatter sections
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Nostr filters that subscribe to relay data
    #[serde(default)]
    pub filters: HashMap<String, Filter>,
    /// jq transformations that pipe filter results
    #[serde(default)]
    pub pipes: HashMap<String, Pipe>,
    /// Nostr event templates for publishing
    #[serde(default)]
    pub actions: HashMap<String, Action>,
    /// App-local state with initial values
    #[serde(default)]
    pub state: HashMap<String, serde_json::Value>,
}

impl Frontmatter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(mut self, id: impl Into<String>, filter: Filter) -> Self {
        self.filters.insert(id.into(), filter);
        self
    }

    pub fn with_pipe(mut self, id: impl Into<String>, pipe: Pipe) -> Self {
        self.pipes.insert(id.into(), pipe);
        self
    }

    pub fn with_action(mut self, id: impl Into<String>, action: Action) -> Self {
        self.actions.insert(id.into(), action);
        self
    }

    pub fn with_state(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.state.insert(key.into(), value);
        self
    }
}

/// Nostr filter definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    /// Event kinds to filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<u64>>,
    /// Author pubkeys (can be template strings like "user.pubkey")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    /// IDs to filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    /// Event IDs referenced in 'e' tags
    #[serde(rename = "#e", skip_serializing_if = "Option::is_none")]
    pub e_tags: Option<Vec<String>>,
    /// Pubkeys referenced in 'p' tags
    #[serde(rename = "#p", skip_serializing_if = "Option::is_none")]
    pub p_tags: Option<Vec<String>>,
    /// Custom tag filters
    #[serde(flatten, skip_serializing_if = "HashMap::is_empty", default)]
    pub custom_tags: HashMap<String, Vec<String>>,
    /// Timestamp lower bound
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// Timestamp upper bound
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Maximum number of events
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl Filter {
    pub fn new() -> Self {
        Self {
            kinds: None,
            authors: None,
            ids: None,
            e_tags: None,
            p_tags: None,
            custom_tags: HashMap::new(),
            since: None,
            until: None,
            limit: None,
        }
    }

    pub fn kinds(mut self, kinds: Vec<u64>) -> Self {
        self.kinds = Some(kinds);
        self
    }

    pub fn authors(mut self, authors: Vec<String>) -> Self {
        self.authors = Some(authors);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new()
    }
}

/// jq transformation pipeline
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipe {
    /// Source filter or pipe ID
    pub from: String,
    /// jq expression to transform the data
    pub jq: String,
}

impl Pipe {
    pub fn new(from: impl Into<String>, jq: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            jq: jq.into(),
        }
    }
}

/// Nostr event template for publishing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Event kind
    pub kind: u64,
    /// Event content (can contain {template} expressions)
    pub content: String,
    /// Event tags
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
}

impl Action {
    pub fn new(kind: u64, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            tags: Vec::new(),
        }
    }

    pub fn with_tag(mut self, tag: Vec<String>) -> Self {
        self.tags.push(tag);
        self
    }
}

/// AST node representing markdown or component
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Node {
    // Markdown nodes
    /// Heading with level 1-6
    Heading {
        level: u8,
        children: Vec<Node>,
    },
    /// Paragraph
    Paragraph {
        children: Vec<Node>,
    },
    /// Plain text
    Text {
        value: String,
    },
    /// Bold text
    Strong {
        children: Vec<Node>,
    },
    /// Italic text
    Emphasis {
        children: Vec<Node>,
    },
    /// Unordered list
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    /// Link
    Link {
        url: String,
        children: Vec<Node>,
    },
    /// Image
    Image {
        src: String,
        alt: String,
    },

    // Expression interpolation
    /// Expression that evaluates at runtime: {queries.feed[0].content}
    Expr {
        expression: String,
    },

    // Component nodes
    /// Iteration over array
    Each {
        /// Expression that evaluates to array
        from: String,
        /// Variable name for iteration
        as_name: String,
        children: Vec<Node>,
    },
    /// Conditional rendering
    If {
        /// Expression to evaluate for truthiness
        value: String,
        /// Nodes to render when truthy
        children: Vec<Node>,
        /// Optional nodes to render when falsy
        #[serde(skip_serializing_if = "Option::is_none")]
        else_children: Option<Vec<Node>>,
    },
    /// Button component
    Button {
        /// Action ID to execute on click
        #[serde(skip_serializing_if = "Option::is_none")]
        on_click: Option<String>,
        children: Vec<Node>,
    },
    /// Text input field
    Input {
        /// Form field name
        name: String,
        /// Placeholder text
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    /// Vertical stack layout
    VStack {
        children: Vec<Node>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        flex: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        align: Option<String>,  // "start", "center", "end"
    },
    /// Horizontal stack layout
    HStack {
        children: Vec<Node>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        flex: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        align: Option<String>,
    },
    /// Grid layout
    Grid {
        /// Number of columns
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<usize>,
        children: Vec<Node>,
    },
    /// JSON debug viewer
    Json {
        /// Expression that evaluates to any value
        value: String,
    },
    /// Spacer for layout
    Spacer {
        /// Size in pixels
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<f64>,
    },
}

/// List item node
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub children: Vec<Node>,
}

impl Node {
    /// Create a heading node
    pub fn heading(level: u8, children: Vec<Node>) -> Self {
        Node::Heading { level, children }
    }

    /// Create a paragraph node
    pub fn paragraph(children: Vec<Node>) -> Self {
        Node::Paragraph { children }
    }

    /// Create a text node
    pub fn text(value: impl Into<String>) -> Self {
        Node::Text {
            value: value.into(),
        }
    }

    /// Create a strong node
    pub fn strong(children: Vec<Node>) -> Self {
        Node::Strong { children }
    }

    /// Create an emphasis node
    pub fn emphasis(children: Vec<Node>) -> Self {
        Node::Emphasis { children }
    }

    /// Create an expression node
    pub fn expr(expression: impl Into<String>) -> Self {
        Node::Expr {
            expression: expression.into(),
        }
    }

    /// Create an each node
    pub fn each(from: impl Into<String>, as_name: impl Into<String>, children: Vec<Node>) -> Self {
        Node::Each {
            from: from.into(),
            as_name: as_name.into(),
            children,
        }
    }

    /// Create an if node
    pub fn if_node(value: impl Into<String>, children: Vec<Node>) -> Self {
        Node::If {
            value: value.into(),
            children,
            else_children: None,
        }
    }

    /// Create an if node with else branch
    pub fn if_else(
        value: impl Into<String>,
        children: Vec<Node>,
        else_children: Vec<Node>,
    ) -> Self {
        Node::If {
            value: value.into(),
            children,
            else_children: Some(else_children),
        }
    }

    /// Create a button node
    pub fn button(on_click: Option<String>, children: Vec<Node>) -> Self {
        Node::Button { on_click, children }
    }

    /// Create an input node
    pub fn input(name: impl Into<String>) -> Self {
        Node::Input {
            name: name.into(),
            placeholder: None,
        }
    }

    /// Create a vstack node
    pub fn vstack(children: Vec<Node>) -> Self {
        Node::VStack {
            children,
            width: None,
            height: None,
            flex: None,
            align: None,
        }
    }

    /// Create an hstack node
    pub fn hstack(children: Vec<Node>) -> Self {
        Node::HStack {
            children,
            width: None,
            height: None,
            flex: None,
            align: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new(Frontmatter::new(), vec![Node::text("Hello")]);
        assert_eq!(doc.version, "1.0.0");
        assert_eq!(doc.body.len(), 1);
    }

    #[test]
    fn test_frontmatter_builder() {
        let fm = Frontmatter::new()
            .with_filter(
                "feed",
                Filter::new().kinds(vec![1]).limit(20),
            )
            .with_pipe("feed_content", Pipe::new("feed", "map(.content)"))
            .with_action("post", Action::new(1, "Hello World"));

        assert_eq!(fm.filters.len(), 1);
        assert_eq!(fm.pipes.len(), 1);
        assert_eq!(fm.actions.len(), 1);
    }

    #[test]
    fn test_node_builders() {
        let heading = Node::heading(1, vec![Node::text("Title")]);
        let paragraph = Node::paragraph(vec![Node::text("Content")]);
        let each = Node::each("queries.feed", "item", vec![Node::text("Item")]);

        assert!(matches!(heading, Node::Heading { level: 1, .. }));
        assert!(matches!(paragraph, Node::Paragraph { .. }));
        assert!(matches!(each, Node::Each { .. }));
    }

    #[test]
    fn test_serde_roundtrip() {
        let doc = Document::new(
            Frontmatter::new().with_filter("test", Filter::new().kinds(vec![1])),
            vec![Node::heading(1, vec![Node::text("Test")])],
        );

        let json = serde_json::to_string(&doc).unwrap();
        let parsed: Document = serde_json::from_str(&json).unwrap();

        assert_eq!(doc, parsed);
    }
}
