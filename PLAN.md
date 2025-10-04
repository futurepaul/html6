# HNMD Rust Implementation Plan

## 📊 Current Progress Summary

**Completed Phases:**
- ✅ Phase 0: Project Setup
- ✅ Phase 1: Parser Foundation (all 5 sub-phases)
- ✅ Phase 2: Static Rendering (all 3 sub-phases)

**Test Status:** 71/73 tests passing

**What Works:**
- Full .hnmd parsing (frontmatter + markdown + JSX components)
- Static rendering to native Masonry widgets
- All markdown features (headings, paragraphs, lists, bold, italic, links, images)
- Layout components (vstack, hstack, grid, spacer)
- Interactive component shells (buttons, inputs) - awaiting runtime for actions
- Roundtrip decompilation (AST → hnmd → AST)

**Next Up:** Phase 3 - Expression Evaluation with jaq

---

## Architecture Decisions (Finalized)

✅ **Parsing**: markdown-rs + custom JSX parser (Option A)
✅ **Attribute Syntax**: Single curly braces `from={queries.feed}` (JSX-style)
✅ **Frontmatter**: Four sections (filters, pipes, actions, state)
✅ **Reactive Runtime**: tokio + async-stream + broadcast channels
✅ **Pipe Engine**: jaq (pure Rust jq implementation)
✅ **Component Rendering**: Direct Masonry with keyed components for smart updates
✅ **Expression Evaluation**: jaq for all expressions (consistent!)
✅ **Nostr Library**: nostr-sdk
✅ **No `$` prefix**: Except in actual jq syntax within pipes

---

## Core Design Principles

1. **One expression language**: Use jaq for pipes AND inline expressions
2. **Consistent interpolation**: `{expr}` in components, same mental model everywhere
3. **Live subscriptions**: Everything is a Nostr filter, including component imports
4. **Keyed updates**: Smart re-rendering to preserve input focus and state
5. **Roundtrip compatibility**: hnmd → AST → hnmd should preserve semantics

---

## Phase-by-Phase Implementation

### Phase 0: Project Setup ✅

**Status**: Complete (basic Rust + Masonry hello world works)

---

### Phase 1: Parser Foundation

**Goal**: Parse .hnmd files to AST with roundtrip tests

#### 1.1: AST Definitions ✅

Create core AST types:

```rust
// src/parser/ast.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub version: String,
    pub frontmatter: Frontmatter,
    pub body: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub filters: HashMap<String, Filter>,
    pub pipes: HashMap<String, Pipe>,
    pub actions: HashMap<String, Action>,
    pub state: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub kinds: Option<Vec<u64>>,
    pub authors: Option<Vec<String>>,  // Can contain template strings
    pub limit: Option<usize>,
    // ... other Nostr filter fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipe {
    pub from: String,  // Filter ID to pipe from
    pub jq: String,    // jq expression
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub kind: u64,
    pub content: String,  // Can contain {templates}
    pub tags: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node {
    // Markdown nodes
    Heading { level: u8, children: Vec<Node> },
    Paragraph { children: Vec<Node> },
    Text { value: String },
    Strong { children: Vec<Node> },
    Emphasis { children: Vec<Node> },

    // Expression interpolation
    Expr { expression: String },  // {queries.feed[0].content}

    // Component nodes
    Each { from: String, as_name: String, children: Vec<Node> },
    If { value: String, children: Vec<Node>, else_children: Option<Vec<Node>> },
    Button { on_click: Option<String>, children: Vec<Node> },
    Input { name: String, placeholder: Option<String> },
    VStack { children: Vec<Node> },
    HStack { children: Vec<Node> },
}
```

**Tasks**:
- [x] Define AST types in `src/parser/ast.rs`
- [x] Add serde serialization/deserialization
- [x] Add builder methods for ergonomic AST construction

**Dependencies**: `serde`, `serde_json`, `serde_yaml` ✅

---

#### 1.2: Frontmatter Parser ✅

Parse YAML frontmatter into AST:

```rust
// src/parser/frontmatter.rs

pub fn parse_frontmatter(yaml: &str) -> Result<Frontmatter> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml)?;

    Ok(Frontmatter {
        filters: parse_filters(value.get("filters"))?,
        pipes: parse_pipes(value.get("pipes"))?,
        actions: parse_actions(value.get("actions"))?,
        state: parse_state(value.get("state"))?,
    })
}

fn parse_filters(value: Option<&Value>) -> Result<HashMap<String, Filter>> {
    // Parse each filter, validate Nostr filter structure
}
```

**Tasks**:
- [x] Implement `parse_frontmatter()`
- [x] Validate filter fields (kinds, authors, etc.)
- [x] Handle template strings in filter values (preserve as-is for now)
- [x] Parse pipe definitions with jq strings
- [x] Parse action templates

**Tests**:
- [x] Parse valid frontmatter
- [x] Reject invalid YAML
- [x] Handle missing sections (all optional)

---

#### 1.3: Markdown + JSX Parser ✅

Parse markdown body with JSX components:

**Approach**:
1. Use `markdown-rs` to parse markdown
2. Hook into HTML nodes to detect components
3. Parse component attributes and recursively parse children as markdown

```rust
// src/parser/markdown.rs

use markdown::mdast::{Node as MdNode, Html};

pub fn parse_body(markdown: &str) -> Result<Vec<Node>> {
    let mdast = markdown::to_mdast(markdown, &markdown::ParseOptions::default())?;
    transform_mdast(mdast)
}

fn transform_mdast(node: MdNode) -> Result<Vec<Node>> {
    match node {
        MdNode::Heading(h) => {
            Ok(vec![Node::Heading {
                level: h.depth,
                children: transform_children(h.children)?,
            }])
        }
        MdNode::Paragraph(p) => {
            Ok(vec![Node::Paragraph {
                children: transform_children(p.children)?,
            }])
        }
        MdNode::Text(t) => {
            // Check for {expression} patterns
            parse_text_with_expressions(&t.value)
        }
        MdNode::Html(html) => {
            // Detect if this is a component tag
            if is_component_tag(&html.value) {
                parse_component(&html.value)
            } else {
                // Regular HTML, preserve or error
                Err(Error::UnsupportedHtml)
            }
        }
        // ... other markdown nodes
    }
}

fn is_component_tag(html: &str) -> bool {
    html.trim().starts_with("<each")
        || html.trim().starts_with("<if")
        || html.trim().starts_with("<button")
        // ... etc
}

fn parse_component(html: &str) -> Result<Vec<Node>> {
    // Custom component parser
    // Handle: <each from={queries.feed} as="note">
    // Extract tag name, attributes, children

    let component = ComponentParser::parse(html)?;

    match component.tag.as_str() {
        "each" => {
            let from = component.attrs.get("from").required()?;
            let as_name = component.attrs.get("as").required()?;
            let children = parse_body(&component.inner_html)?;

            Ok(vec![Node::Each { from, as_name, children }])
        }
        "if" => {
            // Handle if/else branches
            // Need to detect </if> and parse children
        }
        // ... other components
    }
}
```

**Custom Component Parser**:

Since `markdown-rs` gives us raw HTML strings, we need a simple component parser:

```rust
// src/parser/component.rs

struct ComponentParser;

impl ComponentParser {
    fn parse(html: &str) -> Result<Component> {
        // Parse: <tag attr1={expr} attr2="string">children</tag>
        // Handle self-closing: <input name="foo" />
        // Extract attributes with {} expressions
    }
}

struct Component {
    tag: String,
    attrs: HashMap<String, AttrValue>,
    inner_html: String,
    self_closing: bool,
}

enum AttrValue {
    Literal(String),      // as="note"
    Expression(String),   // from={queries.feed}
}
```

**Tasks**:
- [x] Implement markdown → AST transformation
- [x] Parse supported markdown subset (headings, paragraphs, bold, italic, lists)
- [x] Detect component tags in HTML nodes
- [x] Implement basic component parser (tag name, attributes)
- [x] Handle attribute expressions `{expr}` vs string literals
- [x] Recursively parse component children as markdown
- [x] Handle self-closing tags (`<input />`)

**Tests**:
- [x] Parse pure markdown (headings, paragraphs, bold)
- [x] Parse component tags with attributes
- [x] Parse nested components (`<each>` containing `<if>`)
- [x] Parse expressions in text: `Hello {user.name}`
- [x] Error on unsupported markdown (tables, etc.)

---

#### 1.4: Expression Parser ✅

Parse expression strings for validation and analysis:

```rust
// src/parser/expr.rs

#[derive(Debug, Clone)]
pub enum Expr {
    Path(Vec<String>),           // queries.feed[0].content
    Literal(serde_json::Value),  // "string" or 123
    JqExpr(String),              // Complex jq expression
}

pub fn parse_expr(s: &str) -> Result<Expr> {
    // For now, just validate it's parseable by jaq
    // Store as string, evaluate at runtime

    // Check if it's a simple path: queries.feed
    // Or literal: "string" / 123
    // Otherwise: full jq expression
}
```

**Tasks**:
- [x] Parse and validate expressions
- [x] Detect simple paths vs complex jq
- [x] Store original expression string for runtime eval

---

#### 1.5: Roundtrip Serialization ✅

Implement AST → .hnmd decompiler:

```rust
// src/parser/decompile.rs

pub fn decompile(doc: &Document) -> String {
    let mut output = String::new();

    // Write frontmatter
    output.push_str("---\n");
    output.push_str(&serialize_frontmatter(&doc.frontmatter));
    output.push_str("---\n\n");

    // Write body
    for node in &doc.body {
        output.push_str(&decompile_node(node));
    }

    output
}

fn decompile_node(node: &Node) -> String {
    match node {
        Node::Heading { level, children } => {
            format!("{} {}\n\n", "#".repeat(*level as usize), decompile_children(children))
        }
        Node::Each { from, as_name, children } => {
            format!(
                "<each from={{{}}} as=\"{}\">\n{}</each>\n",
                from, as_name, decompile_children(children)
            )
        }
        // ... other nodes
    }
}
```

**Tasks**:
- [x] Implement decompiler for all node types
- [x] Format frontmatter as clean YAML
- [x] Preserve expression syntax exactly

**Tests**:
- [x] Roundtrip: parse → decompile → parse (assert equal AST)
- [x] Formatting is clean and readable

---

### Phase 2: Static Rendering (No Data) ✅

**Goal**: Render parsed AST to Masonry widgets (static content only)

#### 2.1: File Loader ✅

Load .hnmd files from `apps/` directory:

```rust
// src/loader.rs

pub fn load_hnmd(path: &str) -> Result<Document> {
    let content = std::fs::read_to_string(path)?;
    parse_hnmd(&content)
}
```

**Tasks**:
- [x] Implement file loader
- [x] Parse on load
- [x] Handle errors gracefully

---

#### 2.2: Widget Builder (Static) ✅

Render AST nodes to Masonry widgets:

```rust
// src/renderer/widgets.rs

use masonry::widgets::*;
use masonry::core::Widget;

pub fn build_widget(node: &Node) -> Box<dyn Widget> {
    match node {
        Node::Heading { level, children } => {
            let text = render_children_to_string(children);
            let size = match level {
                1 => 32.0,
                2 => 24.0,
                3 => 18.0,
                _ => 16.0,
            };
            Box::new(
                Label::new(text)
                    .with_style(StyleProperty::FontSize(size))
                    .with_style(StyleProperty::FontWeight(FontWeight::BOLD))
            )
        }
        Node::Paragraph { children } => {
            let text = render_children_to_string(children);
            Box::new(Label::new(text))
        }
        Node::VStack { children } => {
            let mut flex = Flex::column();
            for child in children {
                flex = flex.with_child(build_widget(child));
            }
            Box::new(flex)
        }
        Node::Button { children, .. } => {
            let text = render_children_to_string(children);
            Box::new(Button::with_text(text))
        }
        // Skip data-dependent nodes for now (Each, If, Expr)
        Node::Each { .. } => Box::new(Label::new("[each - not rendered yet]")),
        Node::Expr { .. } => Box::new(Label::new("[expr - not rendered yet]")),
        _ => Box::new(Label::new("[unsupported]")),
    }
}

fn render_children_to_string(children: &[Node]) -> String {
    children.iter().map(node_to_string).collect::<Vec<_>>().join("")
}

fn node_to_string(node: &Node) -> String {
    match node {
        Node::Text { value } => value.clone(),
        Node::Strong { children } => render_children_to_string(children), // Ignore formatting for now
        Node::Emphasis { children } => render_children_to_string(children),
        _ => String::new(),
    }
}
```

**Tasks**:
- [x] Implement widget builders for markdown nodes (heading, paragraph, text)
- [x] Implement layout components (vstack, hstack)
- [x] Implement button (no action yet)
- [x] Stub out data-dependent nodes (each, if, expr)
- [x] Handle bold/italic (might need rich text or ignore for now)

**Tests**:
- [x] Render static hello world app
- [x] Render headings with correct sizes
- [x] Render layouts (vstack, hstack)

---

#### 2.3: Main App Integration ✅

Wire up file loading to Masonry window:

```rust
// src/main.rs

use masonry::core::{NewWidget, Widget as _};
use masonry_winit::app::{AppDriver, DriverCtx, NewWindow, WindowId};

struct HnmdDriver {
    window_id: WindowId,
}

impl AppDriver for HnmdDriver {
    fn on_action(&mut self, window_id: WindowId, _ctx: &mut DriverCtx, _widget_id: WidgetId, _action: ErasedAction) {
        debug_assert_eq!(window_id, self.window_id);
    }
}

fn main() {
    // Load app
    let doc = load_hnmd("apps/hello.hnmd").expect("Failed to load app");

    // Build widget tree
    let root_widget = build_document_widget(&doc);

    // Launch window
    let driver = HnmdDriver {
        window_id: WindowId::next(),
    };

    masonry_winit::app::run(
        masonry_winit::app::EventLoop::with_user_event(),
        vec![NewWindow::new_with_id(
            driver.window_id,
            Window::default_attributes().with_title("HNMD Viewer"),
            NewWidget::new(root_widget).erased(),
        )],
        driver,
        default_property_set(),
    )
    .unwrap();
}

fn build_document_widget(doc: &Document) -> impl Widget {
    let mut vstack = Flex::column();
    for node in &doc.body {
        vstack = vstack.with_child(build_widget(node));
    }
    vstack
}
```

**Tasks**:
- [x] Load hello.hnmd from apps directory
- [x] Render to window
- [x] Display static content

**Test App** (`apps/hello.hnmd`):
```hnmd
# Hello World

This is a **static** test.

<vstack>
  ## Subheading

  Some paragraph text.

  <button>Click me (does nothing yet)</button>
</vstack>
```

---

### Phase 3: Expression Evaluation with jaq

**Goal**: Evaluate expressions using jaq, build runtime context

#### 3.1: jaq Integration

Wrapper around jaq for expression evaluation:

```rust
// src/runtime/jaq.rs

use jaq_interpret::{Ctx, FilterT, RcIter, Val};
use serde_json::Value;

pub struct JaqEvaluator {
    // Cache compiled filters
    cache: HashMap<String, jaq_interpret::Filter>,
}

impl JaqEvaluator {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    pub fn eval(&mut self, expr: &str, context: &Value) -> Result<Value> {
        // Compile jq expression (or get from cache)
        let filter = self.compile(expr)?;

        // Convert context to jaq Val
        let val = Val::from(context.clone());

        // Execute
        let inputs = RcIter::new(core::iter::empty());
        let result = filter
            .run((Ctx::new([], &inputs), val))
            .next()
            .ok_or(Error::NoResult)??;

        // Convert back to serde_json::Value
        Ok(val_to_json(result))
    }

    fn compile(&mut self, expr: &str) -> Result<jaq_interpret::Filter> {
        if let Some(cached) = self.cache.get(expr) {
            return Ok(cached.clone());
        }

        let mut defs = jaq_interpret::ParseCtx::new(Vec::new());
        let (filter, errs) = jaq_parse::parse(expr, jaq_parse::main());

        if !errs.is_empty() {
            return Err(Error::JaqParse(errs));
        }

        let filter = defs.compile(filter.ok_or(Error::NoFilter)?);
        self.cache.insert(expr.to_string(), filter.clone());

        Ok(filter)
    }
}

fn val_to_json(val: Val) -> Value {
    // Convert jaq Val to serde_json::Value
    // Handle strings, numbers, bools, arrays, objects, null
}
```

**Tasks**:
- [ ] Implement jaq wrapper with caching
- [ ] Handle jaq compilation errors gracefully
- [ ] Convert between jaq Val and serde_json::Value
- [ ] Test basic expressions: `.user.name`, `.queries.feed[0].content`
- [ ] Test fallback operator: `.user.name // "Anon"`

**Tests**:
- [ ] Eval simple path: `.name` with `{"name": "Alice"}` → `"Alice"`
- [ ] Eval array access: `.[0]` with `[1,2,3]` → `1`
- [ ] Eval fallback: `.missing // "default"` → `"default"`
- [ ] Eval complex: `.[] | select(.age > 18)` → filter array

---

#### 3.2: Runtime Context Builder

Build the context object that expressions evaluate against:

```rust
// src/runtime/context.rs

#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub user: Value,      // {pubkey, profile}
    pub queries: Value,   // {feed: [...], profile: {...}}
    pub state: Value,     // App state
    pub form: Value,      // Form values
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self {
            user: json!({}),
            queries: json!({}),
            state: json!({}),
            form: json!({}),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "user": self.user,
            "queries": self.queries,
            "state": self.state,
            "form": self.form,
        })
    }

    pub fn eval(&self, expr: &str, evaluator: &mut JaqEvaluator) -> Result<Value> {
        // Prepend `.` if not present
        let jq_expr = if expr.starts_with('.') {
            expr.to_string()
        } else {
            format!(".{}", expr)
        };

        evaluator.eval(&jq_expr, &self.to_json())
    }
}
```

**Tasks**:
- [ ] Define context structure
- [ ] Build context from app state
- [ ] Evaluate expressions against context

---

#### 3.3: Expression Rendering

Update widget builder to evaluate expressions:

```rust
// src/renderer/widgets.rs (updated)

pub struct RenderContext {
    pub runtime_ctx: RuntimeContext,
    pub evaluator: JaqEvaluator,
}

pub fn build_widget_with_context(node: &Node, ctx: &mut RenderContext) -> Box<dyn Widget> {
    match node {
        Node::Expr { expression } => {
            // Evaluate expression
            let result = ctx.runtime_ctx.eval(expression, &mut ctx.evaluator)
                .unwrap_or_else(|_| json!("[error]"));

            let text = value_to_string(&result);
            Box::new(Label::new(text))
        }
        Node::Paragraph { children } => {
            let text = render_children_with_context(children, ctx);
            Box::new(Label::new(text))
        }
        // ... other nodes updated to use context
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string_pretty(value).unwrap(),
    }
}
```

**Tasks**:
- [ ] Update widget builders to accept RenderContext
- [ ] Evaluate Expr nodes
- [ ] Evaluate expressions in text nodes: `Hello {user.name}`
- [ ] Handle eval errors gracefully

**Test**:
- [ ] Render `{user.name}` with context `{"user": {"name": "Alice"}}` → "Alice"
- [ ] Render `Count: {state.count}` → "Count: 0"

---

### Phase 4: Nostr Filters

**Goal**: Subscribe to Nostr relays, populate query context

#### 4.1: Nostr Client Setup

Initialize nostr-sdk client:

```rust
// src/nostr/client.rs

use nostr_sdk::prelude::*;

pub struct NostrClient {
    client: Client,
}

impl NostrClient {
    pub async fn new(relays: Vec<String>) -> Result<Self> {
        let keys = Keys::generate(); // Or load from NIP-07
        let client = Client::new(&keys);

        for relay in relays {
            client.add_relay(&relay).await?;
        }

        client.connect().await;

        Ok(Self { client })
    }

    pub async fn subscribe(&self, filter: Filter) -> RelaySubscription {
        self.client.subscribe(vec![filter]).await
    }
}
```

**Tasks**:
- [ ] Initialize nostr-sdk Client
- [ ] Connect to default relays (wss://relay.damus.io, etc.)
- [ ] Handle key generation/loading
- [ ] Basic subscription API

**Dependencies**: `nostr-sdk = "0.37"`

---

#### 4.2: Filter Compilation

Convert frontmatter filters to nostr-sdk Filters:

```rust
// src/nostr/filters.rs

use nostr_sdk::prelude::*;
use crate::parser::ast;

pub fn compile_filter(filter_def: &ast::Filter, ctx: &RuntimeContext) -> Result<Filter> {
    let mut filter = Filter::new();

    if let Some(kinds) = &filter_def.kinds {
        let kinds: Vec<Kind> = kinds.iter()
            .map(|k| Kind::from(*k))
            .collect();
        filter = filter.kinds(kinds);
    }

    if let Some(authors) = &filter_def.authors {
        // Resolve template strings: "user.pubkey" → actual pubkey
        let pubkeys: Vec<PublicKey> = authors.iter()
            .filter_map(|a| resolve_pubkey(a, ctx).ok())
            .collect();
        filter = filter.authors(pubkeys);
    }

    if let Some(limit) = filter_def.limit {
        filter = filter.limit(limit);
    }

    Ok(filter)
}

fn resolve_pubkey(template: &str, ctx: &RuntimeContext) -> Result<PublicKey> {
    // If it's a hex pubkey, use directly
    if let Ok(pk) = PublicKey::from_hex(template) {
        return Ok(pk);
    }

    // Otherwise, treat as expression: "user.pubkey"
    let value = ctx.eval(template, &mut JaqEvaluator::new())?;
    let hex = value.as_str().ok_or(Error::InvalidPubkey)?;
    PublicKey::from_hex(hex)
}
```

**Tasks**:
- [ ] Convert ast::Filter to nostr_sdk::Filter
- [ ] Resolve template strings in filter fields
- [ ] Handle all filter fields (kinds, authors, tags, since, until, limit)

**Tests**:
- [ ] Compile static filter: `{kinds: [1], limit: 20}`
- [ ] Compile dynamic filter: `{kinds: [0], authors: ["user.pubkey"]}`
- [ ] Error on invalid templates

---

#### 4.3: Query Runtime

Subscribe to filters and populate context:

```rust
// src/runtime/query.rs

use tokio::sync::broadcast;
use std::collections::HashMap;

pub struct QueryRuntime {
    client: NostrClient,
    subscriptions: HashMap<String, broadcast::Sender<Vec<Event>>>,
}

impl QueryRuntime {
    pub fn new(client: NostrClient) -> Self {
        Self {
            client,
            subscriptions: HashMap::new(),
        }
    }

    pub async fn subscribe_filter(&mut self, id: String, filter: Filter) -> broadcast::Receiver<Vec<Event>> {
        let (tx, rx) = broadcast::channel(100);

        let subscription = self.client.subscribe(filter).await;

        // Spawn task to collect events
        tokio::spawn(async move {
            let mut events = Vec::new();

            loop {
                tokio::select! {
                    event = subscription.recv() => {
                        if let Some(event) = event {
                            events.push(event);
                            tx.send(events.clone()).ok();
                        }
                    }
                }
            }
        });

        self.subscriptions.insert(id, tx);
        rx
    }

    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<Vec<Event>>> {
        self.subscriptions.get(id).map(|tx| tx.subscribe())
    }
}
```

**Tasks**:
- [ ] Implement QueryRuntime with tokio tasks
- [ ] Subscribe to each filter from frontmatter
- [ ] Collect events into Vec (maintain sorted by created_at)
- [ ] Broadcast updates via channels
- [ ] Expose receivers for UI consumption

**Tests**:
- [ ] Subscribe to filter, receive events
- [ ] Multiple subscribers to same filter work

---

#### 4.4: Context Population

Update RuntimeContext with query results:

```rust
// src/runtime/context.rs (updated)

impl RuntimeContext {
    pub fn update_query(&mut self, id: &str, events: Vec<Event>) {
        let events_json: Vec<Value> = events.iter()
            .map(|e| event_to_json(e))
            .collect();

        self.queries[id] = json!(events_json);
    }
}

fn event_to_json(event: &Event) -> Value {
    json!({
        "id": event.id.to_hex(),
        "pubkey": event.pubkey.to_hex(),
        "created_at": event.created_at.as_u64(),
        "kind": event.kind.as_u64(),
        "content": event.content,
        "tags": event.tags,
        "sig": event.sig.to_string(),
    })
}
```

**Tasks**:
- [ ] Convert nostr Event to JSON
- [ ] Update context when events arrive
- [ ] Trigger widget rebuild (next phase)

---

### Phase 5: Pipes & Transforms

**Goal**: Apply jq transformations to filter results

#### 5.1: Pipe Execution

Execute pipes on filter results:

```rust
// src/runtime/query.rs (updated)

impl QueryRuntime {
    pub async fn subscribe_pipe(&mut self, id: String, pipe: &Pipe, evaluator: &mut JaqEvaluator) -> broadcast::Receiver<Value> {
        let (tx, rx) = broadcast::channel(100);

        // Get source filter receiver
        let mut source_rx = self.get_receiver(&pipe.from)
            .ok_or(Error::UnknownFilter)?;

        let jq_expr = pipe.jq.clone();

        tokio::spawn(async move {
            loop {
                if let Ok(events) = source_rx.recv().await {
                    // Convert to JSON and apply jq
                    let events_json = json!(events);

                    if let Ok(result) = evaluator.eval(&jq_expr, &events_json) {
                        tx.send(result).ok();
                    }
                }
            }
        });

        self.subscriptions.insert(id, tx);
        rx
    }
}
```

**Tasks**:
- [ ] Subscribe to source filter
- [ ] Apply jq transformation on each update
- [ ] Broadcast transformed results
- [ ] Handle jq errors

**Tests**:
- [ ] Pipe: `from: feed, jq: 'map(.content)'` → array of content strings
- [ ] Pipe: `from: feed, jq: '.[0]'` → first event
- [ ] Pipe: `from: profile, jq: '.[0].content | fromjson'` → parsed profile

---

### Phase 6: Reactive Widget Updates

**Goal**: Rebuild widgets when query data changes

#### 6.1: Widget Keying

Add stable keys to widgets for smart updates:

```rust
// src/renderer/keyed.rs

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct WidgetKey(String);

impl WidgetKey {
    pub fn from_node(node: &Node, index: usize) -> Self {
        match node {
            Node::Input { name, .. } => WidgetKey(format!("input:{}", name)),
            Node::Each { from, .. } => WidgetKey(format!("each:{}:{}", from, index)),
            _ => WidgetKey(format!("node:{}", index)),
        }
    }
}

pub struct KeyedWidget {
    pub key: WidgetKey,
    pub widget: WidgetPod<Box<dyn Widget>>,
    pub node: Node,  // Store original node for comparison
}
```

**Tasks**:
- [ ] Generate stable keys for nodes
- [ ] Store keys with widgets
- [ ] Compare old/new node to detect changes

---

#### 6.2: Smart Rebuild

Only rebuild changed widgets:

```rust
// src/renderer/update.rs

pub fn update_widget_tree(
    old: &[KeyedWidget],
    new_nodes: &[Node],
    ctx: &mut RenderContext,
) -> Vec<KeyedWidget> {
    let mut result = Vec::new();

    for (index, new_node) in new_nodes.iter().enumerate() {
        let new_key = WidgetKey::from_node(new_node, index);

        // Find matching old widget
        if let Some(old_widget) = old.iter().find(|w| w.key == new_key) {
            // Check if node changed
            if nodes_equal(&old_widget.node, new_node) {
                // Reuse widget
                result.push(old_widget.clone());
            } else {
                // Rebuild widget
                result.push(KeyedWidget {
                    key: new_key,
                    widget: build_widget_with_context(new_node, ctx),
                    node: new_node.clone(),
                });
            }
        } else {
            // New widget
            result.push(KeyedWidget {
                key: new_key,
                widget: build_widget_with_context(new_node, ctx),
                node: new_node.clone(),
            });
        }
    }

    result
}

fn nodes_equal(a: &Node, b: &Node) -> bool {
    // Deep equality check
    // For now, simple comparison
    // Could optimize with hash
    a == b
}
```

**Tasks**:
- [ ] Implement update_widget_tree
- [ ] Reuse unchanged widgets
- [ ] Rebuild only changed widgets
- [ ] Handle widget removal (old key not in new)

---

#### 6.3: Reactive Main Loop

Listen to query updates and rebuild UI:

```rust
// src/main.rs (updated)

async fn run_app() {
    let doc = load_hnmd("apps/feed.hnmd").unwrap();

    // Setup Nostr client
    let client = NostrClient::new(vec!["wss://relay.damus.io".into()]).await.unwrap();
    let mut query_runtime = QueryRuntime::new(client);

    // Subscribe to filters
    for (id, filter_def) in &doc.frontmatter.filters {
        let filter = compile_filter(filter_def, &RuntimeContext::new()).unwrap();
        query_runtime.subscribe_filter(id.clone(), filter).await;
    }

    // Build initial context
    let mut ctx = RuntimeContext::new();
    let mut render_ctx = RenderContext {
        runtime_ctx: ctx.clone(),
        evaluator: JaqEvaluator::new(),
    };

    // Build initial widgets
    let mut widgets = build_document_widgets(&doc, &mut render_ctx);

    // Listen for updates
    // TODO: Integrate with Masonry event loop
    // For now, spawn task that listens and marks dirty
    tokio::spawn(async move {
        loop {
            // Check for query updates
            // Update context
            // Mark widgets dirty
            // Trigger rebuild
        }
    });

    // Launch Masonry window (as before)
}
```

**Challenge**: Masonry doesn't have built-in reactive updates. We need to:
1. Keep widget tree in app state
2. Listen to query updates
3. Send "rebuild" message to Masonry
4. Rebuild widget tree on message

**Needs research**: How to trigger Masonry repaints from external events.

**Tasks**:
- [ ] Integrate tokio runtime with Masonry event loop
- [ ] Trigger widget rebuilds on query updates
- [ ] Update window with new widget tree

---

### Phase 7: Control Flow Components

**Goal**: Implement `<each>` and `<if>` components

#### 7.1: Each Component

Render lists:

```rust
// src/renderer/widgets.rs (updated)

pub fn build_widget_with_context(node: &Node, ctx: &mut RenderContext) -> Box<dyn Widget> {
    match node {
        Node::Each { from, as_name, children } => {
            // Evaluate `from` expression to get array
            let items = ctx.runtime_ctx.eval(from, &mut ctx.evaluator)
                .and_then(|v| v.as_array().ok_or(Error::NotArray))
                .unwrap_or(&vec![]);

            let mut vstack = Flex::column();

            for (index, item) in items.iter().enumerate() {
                // Create scoped context with `item` binding
                let mut scoped_ctx = ctx.clone();
                scoped_ctx.runtime_ctx.add_local(as_name, item.clone());
                scoped_ctx.runtime_ctx.add_local("itemIndex", json!(index));

                // Render children with scoped context
                for child in children {
                    vstack = vstack.with_child(
                        build_widget_with_context(child, &mut scoped_ctx)
                    );
                }
            }

            Box::new(vstack)
        }
        // ... other nodes
    }
}
```

**Tasks**:
- [ ] Evaluate `from` expression
- [ ] Iterate over array
- [ ] Create scoped context with item binding
- [ ] Render children for each item
- [ ] Add `itemIndex` binding

**Tests**:
- [ ] Render each with static array: `from={[1,2,3]}`
- [ ] Render each with query: `from={queries.feed}`
- [ ] Access item in child: `{note.content}`
- [ ] Access itemIndex: `{itemIndex}`

---

#### 7.2: If Component

Conditional rendering:

```rust
Node::If { value, children, else_children } => {
    // Evaluate condition
    let condition = ctx.runtime_ctx.eval(value, &mut ctx.evaluator)
        .map(|v| is_truthy(&v))
        .unwrap_or(false);

    let branches = if condition { children } else { else_children.as_ref().unwrap_or(&vec![]) };

    let mut vstack = Flex::column();
    for child in branches {
        vstack = vstack.with_child(build_widget_with_context(child, ctx));
    }

    Box::new(vstack)
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
```

**Tasks**:
- [ ] Evaluate condition expression
- [ ] Render truthy or falsy branch
- [ ] Handle missing else branch

**Tests**:
- [ ] Render when true: `<if value={true}>shown</if>`
- [ ] Hide when false: `<if value={false}>hidden</if>`
- [ ] Render else: `<if value={false}>no<else>yes</else></if>`

---

### Phase 8: Actions & Interactivity

**Goal**: Execute actions when buttons clicked

#### 8.1: Action Compilation

Convert action definitions to publishable events:

```rust
// src/runtime/actions.rs

use nostr_sdk::prelude::*;

pub fn compile_action(
    action_def: &ast::Action,
    ctx: &RuntimeContext,
    evaluator: &mut JaqEvaluator,
) -> Result<EventBuilder> {
    let kind = Kind::from(action_def.kind);

    // Interpolate content
    let content = interpolate_template(&action_def.content, ctx, evaluator)?;

    // Build event
    let mut builder = EventBuilder::new(kind, &content, &[]);

    // Add tags
    for tag in &action_def.tags {
        builder = builder.tags(vec![Tag::Generic(
            TagKind::Custom(tag[0].clone()),
            tag[1..].to_vec(),
        )]);
    }

    Ok(builder)
}

fn interpolate_template(template: &str, ctx: &RuntimeContext, evaluator: &mut JaqEvaluator) -> Result<String> {
    // Find all {expr} patterns
    // Evaluate each expression
    // Replace with result

    let re = regex::Regex::new(r"\{([^}]+)\}").unwrap();
    let mut result = template.to_string();

    for cap in re.captures_iter(template) {
        let expr = &cap[1];
        let value = ctx.eval(expr, evaluator)?;
        let replacement = value_to_string(&value);
        result = result.replace(&cap[0], &replacement);
    }

    Ok(result)
}
```

**Tasks**:
- [ ] Interpolate action templates
- [ ] Build EventBuilder
- [ ] Handle tags
- [ ] Support all Nostr event fields

**Tests**:
- [ ] Compile action with static content
- [ ] Compile action with `{form.note}` interpolation
- [ ] Compile action with tags

---

#### 8.2: Button Actions

Wire up button clicks to actions:

**Challenge**: Masonry buttons don't directly support custom actions. We need to:
1. Emit ButtonPress action
2. Capture in AppDriver.on_action
3. Look up which button was pressed
4. Execute corresponding action

```rust
// src/renderer/widgets.rs (updated)

Node::Button { on_click, children } => {
    let text = render_children_to_string(children);
    let button = Button::with_text(text);

    // Store action ID in widget somehow
    // Maybe custom action type?

    Box::new(button)
}

// src/main.rs (updated)

impl AppDriver for HnmdDriver {
    fn on_action(&mut self, window_id: WindowId, ctx: &mut DriverCtx, widget_id: WidgetId, action: ErasedAction) {
        if action.is::<ButtonPress>() {
            // Lookup which button was pressed
            // Get action ID from button
            // Execute action

            let action_id = self.get_action_for_widget(widget_id);
            if let Some(action_id) = action_id {
                self.execute_action(&action_id).await;
            }
        }
    }
}

impl HnmdDriver {
    async fn execute_action(&mut self, action_id: &str) {
        let action_def = self.doc.frontmatter.actions.get(action_id).unwrap();
        let event_builder = compile_action(action_def, &self.runtime_ctx, &mut self.evaluator).unwrap();

        // Sign and publish
        self.nostr_client.publish(event_builder).await.unwrap();
    }
}
```

**Tasks**:
- [ ] Map widget IDs to action IDs
- [ ] Handle ButtonPress action in driver
- [ ] Execute and publish action

**Needs research**: How to associate data with Masonry widgets (action ID).

---

### Phase 9: Forms & More Components

**Goal**: Input fields, checkboxes, full component set

#### 9.1: Input Component

Text input with form binding:

```rust
Node::Input { name, placeholder } => {
    // Create text input widget
    // Bind to form.{name}
    // Update form on change
}
```

**Challenge**: Two-way binding. Need to:
1. Read initial value from `form.{name}`
2. Update form on text change
3. Re-render components that depend on form

**Tasks**:
- [ ] Implement input widget
- [ ] Bind to form state
- [ ] Update form on change
- [ ] Trigger re-render

---

#### 9.2: Layout Components

Grid, spacers, etc.

**Tasks**:
- [ ] Implement Grid layout
- [ ] Implement Spacer
- [ ] Add layout props (gap, align, etc.)

---

### Phase 10: Component Imports (Advanced)

**Goal**: Load components from Nostr

#### 10.1: Component Registry

```rust
pub struct ComponentRegistry {
    cache: HashMap<String, Document>,
}

impl ComponentRegistry {
    pub async fn load_component(&mut self, naddr: &str, client: &NostrClient) -> Result<Document> {
        // Parse naddr
        // Create filter for that event
        // Subscribe
        // Wait for event
        // Parse .hnmd from event content
        // Cache
    }
}
```

**Tasks**:
- [ ] Parse naddr
- [ ] Fetch component via Nostr filter
- [ ] Parse component definition
- [ ] Handle props
- [ ] Scoped filter/pipe registration
- [ ] Render component instance

---

## Testing Strategy

### Unit Tests
- [ ] Parser: Parse valid hnmd → AST
- [ ] Parser: Reject invalid syntax
- [ ] Roundtrip: AST → hnmd → AST (semantic equality)
- [ ] Expressions: Eval with jaq
- [ ] Filters: Compile to nostr-sdk
- [ ] Actions: Interpolate templates

### Integration Tests
- [ ] Load .hnmd file, render static content
- [ ] Subscribe to Nostr, populate queries
- [ ] Execute action, publish event
- [ ] Component imports

### Manual Tests
- [ ] Hello world app renders
- [ ] Nostr feed app shows real events
- [ ] Button publishes note
- [ ] Input updates form state

---

## Dependency Summary

```toml
[dependencies]
# Parsing
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
markdown = "1.0.0-alpha"
regex = "1.10"

# Expression evaluation
jaq-interpret = "1.2"
jaq-parse = "1.0"

# Nostr
nostr-sdk = "0.37"

# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# UI (already have)
masonry = { git = "https://github.com/linebender/xilem" }
masonry_winit = { git = "https://github.com/linebender/xilem" }

# Utilities
anyhow = "1.0"
thiserror = "2.0"
```

---

## Open Questions / Future Research

1. **Masonry reactive updates**: How to trigger rebuilds from tokio tasks?
2. **Widget data association**: How to store action IDs with widgets?
3. **Two-way form binding**: Best pattern for input → form → render cycle?
4. **Performance**: When to diff vs full rebuild?
5. **Error UI**: How to show parse/eval errors in app?

---

## Success Criteria

### Phase 1-3 Complete When:
- [x] Can parse `apps/hello.hnmd` to AST ✅
- [x] Can render static markdown + components ✅
- [ ] Can evaluate expressions: `{user.name}` (not started - Phase 3)

### Phase 4-6 Complete When:
- [ ] Can load Nostr events into queries (not started)
- [ ] Can apply jq pipes (not started)
- [ ] UI updates when events arrive (not started)

### Phase 7-8 Complete When:
- [ ] `<each>` renders lists (not started)
- [ ] `<if>` shows/hides content (not started)
- [ ] Buttons publish events (not started)

### Phase 9 Complete When:
- [ ] Full component set works (partially done - basic components work)
- [ ] Forms are interactive (not started)
- [ ] Can build real Nostr apps (not started - needs runtime)

### Phase 10 Complete When:
- [ ] Can import components by naddr (not started)
- [ ] Component props work (not started)
- [ ] Scoped filters work (not started)

---

## Next Steps

**✅ COMPLETED: Phase 0, Phase 1 (all 5 sub-phases), Phase 2 (all 3 sub-phases)**

The parser and static renderer are fully working! The project can:
- Parse .hnmd files (frontmatter + markdown + JSX components) into AST
- Render static content to Masonry widgets
- Display layouts (vstack, hstack), markdown (headings, paragraphs, lists, bold, italic, links, images)
- Display interactive components (buttons, inputs) - though they don't have actions yet
- 71/73 tests passing

**Current Status:** Ready to begin Phase 3 (Expression Evaluation with jaq)

**Next Phase:** Start with **Phase 3.1: jaq Integration**
1. Add `jaq-interpret` and `jaq-parse` dependencies to Cargo.toml
2. Create `src/runtime/` module structure
3. Implement JaqEvaluator wrapper with caching
4. Build RuntimeContext for expression evaluation

This will enable dynamic content like `{user.name}` and `{queries.feed[0].content}` to work.
