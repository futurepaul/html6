# HNMD Rust Implementation Plan

## Overview

We're building a Rust-native implementation of Hypernote with an improved syntax that combines JSX-style components with Markdown, replacing codeblock syntax with HTML-like tags and improving the frontmatter declaration system.

### Core Improvements from Hypernote-OS

1. **JSX/HTML Syntax** instead of codeblocks (`<each>` instead of ` ```each.start `)
2. **Split Frontmatter**: Separate `filters` (Nostr JSON filters) from `pipes` (jq transformations)
3. **Proper HTML Attribute Binding** for passing data to components
4. **Recursive Markdown Parsing** inside HTML blocks
5. **Declarative Data Flow**: Runtime provides data slots based on filter/pipe declarations

---

## 1. Markdown + JSX Parsing

### Option A: markdown-rs + custom HTML parser

**Approach**: Use `markdown-rs` for standard markdown parsing, intercept HTML blocks, and write a custom JSX-like parser for component tags.

**Pros**:
- `markdown-rs` is CommonMark compliant and fast
- Full control over JSX syntax (can support `{}` attribute expressions)
- Can recursively parse markdown inside component children

**Cons**:
- Need to write custom HTML/JSX parser
- More complex integration between markdown AST and component AST

**Example**:
```rust
use markdown::mdast::{Node, Html};

fn parse_hnmd(source: &str) -> Result<Document> {
    let mdast = markdown::to_mdast(source, &options)?;
    transform_html_to_components(mdast)
}

fn transform_html_to_components(node: Node) -> Node {
    match node {
        Node::Html(html) if is_component(&html.value) => {
            let component = parse_component(&html.value)?;
            // Recursively parse markdown inside children
            Node::Component(component)
        }
        _ => node
    }
}
```

### Option B: scraper + markdown-rs in two passes

**Approach**: First pass extracts JSX components with `scraper` (HTML parser), second pass processes markdown content within each section.

**Pros**:
- `scraper` is a mature HTML parser (based on html5ever)
- Clear separation of concerns
- Can use CSS selectors to find components

**Cons**:
- JSX `{}` expressions might confuse HTML parser
- Two-pass parsing might be slower
- Need to carefully preserve whitespace/structure

**Example**:
```rust
use scraper::{Html, Selector};

fn parse_hnmd(source: &str) -> Result<Document> {
    // First pass: find components
    let fragment = Html::parse_fragment(source);
    let each_selector = Selector::parse("each").unwrap();

    for element in fragment.select(&each_selector) {
        let from = element.value().attr("from");
        let inner = element.inner_html();
        // Parse inner as markdown
        let content_ast = markdown::to_mdast(&inner, &options)?;
    }
}
```

### Option C: Custom unified parser (markdown + JSX in one pass)

**Approach**: Write a custom parser that understands both markdown and JSX syntax simultaneously using a parser combinator library like `nom`.

**Pros**:
- Single source of truth
- Can handle edge cases better
- Full control over syntax

**Cons**:
- Most complex to implement
- Need to maintain markdown compatibility manually
- Higher maintenance burden

**Recommendation Needed**: Which parsing approach do you prefer?

PAUL: I like option A, that seems like a pretty simple integration!

---

## 2. JSX Attribute Expression Syntax

### Option A: Curly brace expressions `{}`

**Syntax**:
```html
<each from={queries.feed} as="item">
  <if value={item.author}>
    {item.content}
  </if>
</each>
```

**Pros**:
- Familiar to React/JSX developers
- Clear distinction between strings and expressions
- Industry standard

**Cons**:
- Harder to parse (need expression parser)
- Conflicts with markdown code blocks `{}`

### Option B: Mustache-style `{{ }}`

**Syntax**:
```html
<each from="{{ queries.feed }}" as="item">
  <if value="{{ item.author }}">
    {{ item.content }}
  </if>
</each>
```

**Pros**:
- Already used in hypernote-os for text interpolation
- Consistent syntax everywhere
- Easier to parse (string replacement)

**Cons**:
- Less idiomatic for component attributes
- Verbose

### Option C: Unquoted expressions (SuperHTML style)

**Syntax**:
```html
<each from=queries.feed as=item>
  <if value=item.author>
    {item.content}
  </if>
</each>
```

**Pros**:
- Clean, minimal syntax
- SuperHTML-inspired (proven design)
- Easy to parse (no quote handling)

**Cons**:
- Non-standard for HTML/JSX
- Might confuse developers expecting quotes

**Recommendation Needed**: Which attribute syntax do you prefer?

PAUL: let's talk more about this. how much harder will markdown parsing be with this? maybe we can look some more at SuperHTML (it uses strings with a $ prefix ctx object)

---

## 3. Frontmatter Structure

### Option A: Four-section frontmatter (filters, pipes, actions, state)

**Syntax**:
```yaml
---
filters:
  feed:
    kinds: [1]
    authors: ["user.pubkey"]
    limit: 20

  profile:
    kinds: [0]
    authors: ["$pubkey"]
    limit: 1

pipes:
  feed_authors:
    from: feed
    jq: '[.[].pubkey] | unique'

  profile_enriched:
    from: profile
    jq: '.content | fromjson'

actions:
  post_note:
    kind: 1
    content: "{{ form.note }}"
    tags: [["client", "hnmd"]]

state:
  selected_pubkey: "user.pubkey"
---
```

**Pros**:
- Clear separation of concerns
- Filters are pure Nostr filters (can be validated)
- Pipes are pure jq (can be sandboxed)
- Easy to understand data flow

**Cons**:
- More verbose
- Need to manage filter→pipe relationships

### Option B: Three-section with inline pipes (queries, actions, state)

**Syntax**:
```yaml
---
queries:
  feed:
    filter:
      kinds: [1]
      authors: ["user.pubkey"]
      limit: 20
    pipe: '[.[].pubkey] | unique'

  profile:
    filter:
      kinds: [0]
      authors: ["$pubkey"]
      limit: 1
    pipe: '.content | fromjson'

actions:
  post_note:
    kind: 1
    content: "{{ form.note }}"

state:
  selected_pubkey: "user.pubkey"
---
```

**Pros**:
- Familiar to hypernote-os users
- Keeps related filter+pipe together
- Less boilerplate

**Cons**:
- Mixing Nostr filters with jq in same block
- Harder to validate separately

### Option C: Explicit dependency graph (filters, transforms, sources)

**Syntax**:
```yaml
---
filters:
  feed: {kinds: [1], authors: ["user.pubkey"], limit: 20}
  profile: {kinds: [0], authors: ["$pubkey"], limit: 1}

transforms:
  feed_authors:
    source: filters.feed
    jq: '[.[].pubkey] | unique'

  profile_data:
    source: filters.profile
    jq: '.content | fromjson'

sources:
  - filters.feed
  - transforms.feed_authors
  - transforms.profile_data

actions:
  post_note:
    kind: 1
    content: "{{ form.note }}"
---
```

**Pros**:
- Explicit about what's exposed to templates
- Clear dependency graph
- Could optimize subscriptions

**Cons**:
- Most verbose
- `sources` section adds complexity

**Recommendation Needed**: Which frontmatter structure do you prefer?

PAUL: I think option A is the easiest to read. one thing to keep in mind though: we need to be consistent here with template replacements (so that the same learnings for the inline markdown syntax work up here) and also watch out for magical `$` prefix. either we have one consistent meaning for what a $ prefix is or we don't use it at all.

---

## 4. Reactive Query Runtime (Hypersauce equivalent)

### Option A: tokio + async-stream + broadcast channels

**Approach**: Use Rust async streams with tokio runtime. Each filter becomes a task that subscribes to relays and broadcasts events through channels. Pipes consume streams and transform them.

**Architecture**:
```rust
struct QueryRuntime {
    filters: HashMap<String, FilterStream>,
    pipes: HashMap<String, PipeStream>,
    subscribers: HashMap<String, broadcast::Sender<Event>>,
}

impl QueryRuntime {
    async fn subscribe_filter(&mut self, id: &str, filter: Filter) {
        let (tx, _rx) = broadcast::channel(100);
        let stream = nostr_client.subscribe(filter).await;

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                tx.send(event).ok();
            }
        });

        self.subscribers.insert(id.into(), tx);
    }

    async fn create_pipe(&mut self, id: &str, config: PipeConfig) {
        let input = self.subscribers.get(&config.from).unwrap().subscribe();
        let jq_program = jq_rs::compile(&config.jq)?;

        let (tx, _rx) = broadcast::channel(100);
        tokio::spawn(async move {
            while let Ok(event) = input.recv().await {
                let result = jq_program.run(&event)?;
                tx.send(result).ok();
            }
        });

        self.subscribers.insert(id.into(), tx);
    }
}
```

**Pros**:
- Native Rust async
- Efficient with tokio ecosystem
- Easy to integrate with Nostr SDK
- broadcast channels handle multiple subscribers naturally

**Cons**:
- Need to manage task lifecycle
- Backpressure handling required
- More complex error handling

### Option B: crossbeam channels + thread-per-filter

**Approach**: Each filter runs in its own thread, communicates via crossbeam channels. Simpler than async but uses more OS threads.

**Architecture**:
```rust
struct QueryRuntime {
    filters: HashMap<String, Receiver<Event>>,
    pipes: HashMap<String, Receiver<Value>>,
}

impl QueryRuntime {
    fn subscribe_filter(&mut self, id: &str, filter: Filter) -> Receiver<Event> {
        let (tx, rx) = crossbeam::channel::unbounded();

        std::thread::spawn(move || {
            let mut client = NostrClient::new();
            client.subscribe(filter);
            loop {
                if let Some(event) = client.recv() {
                    tx.send(event).ok();
                }
            }
        });

        self.filters.insert(id.into(), rx.clone());
        rx
    }
}
```

**Pros**:
- Simpler mental model (no async)
- crossbeam channels are very fast
- Easy to reason about

**Cons**:
- More threads (one per filter)
- Less efficient at scale
- Harder to cancel subscriptions

### Option C: pollster + reactive single-threaded

**Approach**: Use a futures executor on a single thread with a reactive graph library like `reactive-rs` or custom RxRust.

**Pros**:
- Closest to RxJS model from hypernote-os
- Single-threaded = simpler debugging
- Can batch UI updates

**Cons**:
- Need to find/build reactive library
- Might not integrate well with Masonry's event loop
- Less parallelism

**Recommendation Needed**: Which reactive runtime approach do you prefer?

PAUL: optiona A with tokio sounds great as long as we can instrument it well to print out graphs and hook into them to observe the data flow for debugging

---

## 5. JQ Integration for Pipes

### Option A: jq-rs (bindings to C libjq)

**Approach**: Use `jq-rs` crate which wraps the official C implementation.

**Pros**:
- 100% compatible with jq
- Battle-tested
- Fast

**Cons**:
- Requires C dependency
- Harder to sandbox/limit execution
- FFI overhead

### Option B: jaq (pure Rust jq implementation)

**Approach**: Use `jaq` crate, a pure Rust reimplementation of jq.

**Pros**:
- Pure Rust (no C deps)
- Easier to audit/sandbox
- Actively maintained
- Can disable dangerous features

**Cons**:
- Not 100% jq compatible (subset)
- Might be slower than C libjq
- Smaller ecosystem

**Example**:
```rust
use jaq_interpret::{Ctx, Filter, FilterT, RcIter, Val};

fn run_pipe(input: Value, jq_expr: &str) -> Result<Value> {
    let mut defs = jaq_interpret::ParseCtx::new(Vec::new());
    let (filter, errs) = jaq_parse::parse(jq_expr, jaq_parse::main());
    let filter = defs.compile(filter.unwrap());

    let inputs = RcIter::new(core::iter::empty());
    let val = serde_json::from_value(input)?;

    filter
        .run((Ctx::new([], &inputs), Val::from(val)))
        .next()
        .unwrap()
}
```

### Option C: Custom pipe DSL (not jq)

**Approach**: Define our own limited transformation language instead of jq.

**Example syntax**:
```yaml
pipes:
  feed_authors:
    from: feed
    transform:
      - map: .pubkey
      - unique
      - take: 10
```

**Pros**:
- Full control
- Can make it simpler than jq
- Guaranteed safe/non-Turing

**Cons**:
- Need to design language
- Users need to learn new syntax
- Less powerful than jq

**Recommendation Needed**: Which pipe engine do you prefer?

PAUL: option b jaq sounds perfect for us. as long as the user can write normal jq syntax, and then we compile that at runtime and can give compile errors if they use forbidden methods.

---

## 6. Component Set (JSX Elements)

### Proposed Core Components

**Control Flow**:
- `<each from={} as={}>` - Iteration
- `<if value={}>` - Conditional rendering
- `<switch>` with `<case when={}>` and `<default>` - Multi-branch

**Layout**:
- `<vstack>`, `<hstack>` - Flex layouts
- `<grid cols={}>` - Grid layout
- `<spacer size={}>` - Spacing

**Interactive**:
- `<button on_click={}>` - Button
- `<input name={} value={}>` - Text input
- `<checkbox name={} checked={}>` - Checkbox

**Display**:
- `<text>` - Plain text (optional, could just use markdown)
- `<image src={}>` - Image
- Regular markdown (headings, lists, etc.)

PAUL: these look great but we should use markdown when markdown is supported (for text, image... unless parsing is going to be hard). also we should only support a subset of markdown. we don't need tables for instance

### Component Implementation Options

#### Option A: Direct Masonry widget mapping

Each component directly creates a Masonry widget during rendering:

```rust
enum Component {
    Each { from: Expr, as_name: String, children: Vec<Node> },
    Button { on_click: Action, children: Vec<Node> },
    VStack { children: Vec<Node> },
}

impl Component {
    fn render(&self, ctx: &RenderContext) -> Box<dyn Widget> {
        match self {
            Component::Button { on_click, children } => {
                let text = render_children_to_text(children, ctx);
                Box::new(Button::with_text(text))
            }
            Component::VStack { children } => {
                let mut flex = Flex::column();
                for child in children {
                    flex.add_child(child.render(ctx));
                }
                Box::new(flex)
            }
        }
    }
}
```

**Pros**:
- Simple, direct
- No intermediate representation
- Easy to understand

**Cons**:
- Tight coupling to Masonry
- Hard to optimize re-renders
- Can't easily diff changes

#### Option A: Virtual DOM with diffing

Build a virtual tree, diff on updates, only rebuild changed widgets:

```rust
struct VNode {
    id: NodeId,
    component: Component,
    widget: Option<WidgetPod<Box<dyn Widget>>>,
}

fn diff_and_patch(old: &VNode, new: &VNode) -> PatchSet {
    // Calculate minimal changes needed
}

fn apply_patches(root: &mut WidgetPod, patches: PatchSet) {
    // Only update changed widgets
}
```

**Pros**:
- Efficient updates
- Can skip unchanged subtrees
- Better for reactive data

**Cons**:
- More complex
- Need to track widget state
- Masonry might not support partial updates well

**Recommendation Needed**: Direct rendering or virtual DOM approach?

PAUL: can we start with direct rendering and do virtual dom as a future enhancement? my biggest fear is when a re-render might blow away a focused text input or something like that. but I think masonry has ids... hmm let's talk more about this. maybe we need just a little bit of smart updating.

---

## 7. Expression Evaluation

### For attribute expressions like `from={queries.feed}` and text interpolation `{item.content}`

### Option A: Simple path resolver (no operators)

Only support dotted paths and array indexing:

```rust
enum Expr {
    Path(Vec<String>),  // queries.feed.0.content
    Literal(Value),
}

fn eval_expr(expr: &Expr, ctx: &Context) -> Option<Value> {
    match expr {
        Expr::Path(segments) => {
            let mut current = &ctx.root;
            for segment in segments {
                current = current.get(segment)?;
            }
            Some(current.clone())
        }
        Expr::Literal(val) => Some(val.clone()),
    }
}
```

**Pros**:
- Very simple
- Fast
- Safe (no code execution)

**Cons**:
- No operators (`||`, `&&`, `!`)
- No function calls
- Limited expressiveness

### Option B: rhai (embedded scripting)

Use `rhai` for full expression evaluation:

```rust
use rhai::Engine;

let engine = Engine::new();
engine.register_fn("format_date", format_date);

let result: String = engine.eval_with_scope(&mut scope, "item.name || 'Anonymous'")?;
```

**Pros**:
- Full expression language
- Can add custom functions
- Operators, conditionals, etc.

**Cons**:
- More complex
- Security concerns (need to sandbox)
- Might be overkill

### Option C: Custom expression parser (operators only)

Support paths + basic operators (`||`, `&&`, `!`, `==`):

```rust
enum Expr {
    Path(Vec<String>),
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Literal(Value),
}

// Parse: item.author || 'Anon'
// -> Or(Path(["item", "author"]), Literal("Anon"))
```

**Pros**:
- Just enough power
- Safe and controlled
- Fast

**Cons**:
- Need to write parser
- Feature requests will grow it

**Recommendation Needed**: Which expression evaluator?

PAUL: I like option C. we def want paths and `||` fallback... how hard would it be to use jq as our expression parser? does that sound crazy? would we be able to do fallbacks and be able to grab ahold of the correct value? (like our user.name || user.display_name common use case?)

---

## 8. Nostr Integration

### For filters and actions

### Option A: nostr-sdk (most popular Rust Nostr library)

```rust
use nostr_sdk::prelude::*;

let client = Client::new(&keys);
client.add_relay("wss://relay.damus.io").await?;
client.connect().await;

let filter = Filter::new()
    .kind(Kind::TextNote)
    .author(keys.public_key())
    .limit(20);

client.subscribe(vec![filter]).await;

// Listen for events
client.handle_notifications(|notification| async move {
    if let RelayPoolNotification::Event(_url, event) = notification {
        // Process event
    }
});
```

**Pros**:
- Most mature Rust Nostr library
- Active development
- Good docs
- Supports all NIPs

**Cons**:
- Opinionated architecture
- Might be heavyweight for our use case


### Option B: nostr-rs-relay client (minimal)

Use just the protocol parts without full SDK:

**Pros**:
- Lightweight
- More control
- Less dependencies

**Cons**:
- Need to implement more ourselves
- Less community support

**Recommendation Needed**: Which Nostr library?

PAUL: yeah we def want nostr-sdk

---

## 9. File Structure & Module Organization

### Proposed Structure

```
src/
├── main.rs                 # Masonry app entry
├── parser/
│   ├── mod.rs
│   ├── frontmatter.rs      # YAML parsing
│   ├── markdown.rs         # Markdown + JSX parsing
│   ├── expr.rs             # Expression parser
│   └── ast.rs              # AST definitions
├── runtime/
│   ├── mod.rs
│   ├── query.rs            # Filter/pipe runtime
│   ├── context.rs          # Evaluation context
│   └── actions.rs          # Action execution
├── renderer/
│   ├── mod.rs
│   ├── components.rs       # Component implementations
│   └── widgets.rs          # Masonry widget builders
├── nostr/
│   ├── mod.rs
│   ├── client.rs           # Nostr client wrapper
│   └── filters.rs          # Filter builders
└── apps/
    └── hello.hnmd          # Example app

HNMD_RUST.md               # This file
```

---

## 10. Incremental Implementation Plan

### Phase 1: Parser Foundation
- [ ] Frontmatter YAML parsing
- [ ] Basic markdown parsing
- [ ] Simple component parser (just `<button>`, `<text>`)
- [ ] AST definitions

PAUL: we should make sure we have roundtrip tests at this phase (can go from hnmd -> ast -> hnmd, kind of like a prettier pass. should have the same symantic meaning after roundtrip. hypernote-os did this)

### Phase 2: Static Rendering
- [ ] Render parsed AST to Masonry widgets
- [ ] No reactivity, just static hello world
- [ ] File loader for .hnmd files

### Phase 3: Expression Evaluation
- [ ] Path resolver for `{queries.feed}`
- [ ] Context building (queries, state, user)
- [ ] Text interpolation

### Phase 4: Filters & Nostr
- [ ] Nostr client integration
- [ ] Filter execution
- [ ] Event subscription
- [ ] Populate query context

### Phase 5: Pipes & Transforms
- [ ] JQ integration
- [ ] Pipe execution
- [ ] Derived queries (filter → pipe)

### Phase 6: Reactive Updates
- [ ] Query runtime with channels
- [ ] Widget rebuild on data changes
- [ ] Efficient diffing/patching

### Phase 7: Control Flow Components
- [ ] `<each>` iteration
- [ ] `<if>` conditionals
- [ ] Recursive markdown in component children

### Phase 8: Actions & Interactivity
- [ ] Action definitions
- [ ] Button onClick → action execution
- [ ] Event publishing

### Phase 9: Full Component Set
- [ ] Forms (input, checkbox, etc.)
- [ ] Layouts (vstack, hstack, grid)
- [ ] Advanced components

### Phase 10: Polish
- [ ] Error handling
- [ ] Validation
- [ ] Dev tools / debugging
- [ ] Performance optimization

---

## 11. Example App Progression

### Step 1: Static Hello World
```hnmd
# Hello World
```

### Step 2: Static Button
```hnmd
---
state:
  count: 0
---

# Counter

<button>Click me</button>

Count: {state.count}
```

### Step 3: Interactive Counter
```hnmd
---
state:
  count: 0

actions:
  increment:
    state:
      count: "{{ state.count + 1 }}"
---

# Counter

<button on_click={actions.increment}>Click me</button>

Count: {state.count}
```

PAUL: we don't actually support local logic for state mutations like this. the button example needs to do some nostr thing like post a 30078 event with state in it that we can read back... so we probably just want to skip this

### Step 4: Nostr Feed
```hnmd
---
filters:
  feed:
    kinds: [1]
    limit: 10

pipes:
  feed_parsed:
    from: feed
    jq: 'map({author: .pubkey, content: .content, created: .created_at})'
---

# My Feed

<each from={queries.feed_parsed} as="note">
  **{note.author}**: {note.content}
</each>
```

PAUL: beautiful

---

## Questions for You

Please provide your preferences for:

1. **Parsing approach** (Section 1): A, B, or C?
2. **Attribute syntax** (Section 2): A (`{}`), B (`{{ }}`), or C (unquoted)?
3. **Frontmatter structure** (Section 3): A (4-section), B (3-section), or C (explicit graph)?
4. **Reactive runtime** (Section 4): A (tokio async), B (crossbeam threads), or C (single-threaded reactive)?
5. **Pipe engine** (Section 5): A (jq-rs), B (jaq), or C (custom DSL)?
6. **Component rendering** (Section 6): A (direct Masonry), or B (virtual DOM)?
7. **Expression evaluation** (Section 7): A (paths only), B (rhai), or C (custom with operators)?
8. **Nostr library** (Section 8): A (nostr-sdk) or B (minimal)?

Any other architectural preferences or concerns?

PAUL: I put all my answers inline. as for architecture, one thing we will need not at first, but eventually is the ability to publish hypernotes (compile the hnmd to our ast, then publish it to nostr, like hypernote-os does) and load them. so just something to keep in mind.

PAUL: also we didn't address nested components. it should be possible to declare a component import (a nostr filter for a naddr that finds a single nostr note that might be a replaceable event that is hnmd ast) that component might register its own filters and also might take an argument, like a profile component that takes a pubkey as an argument, or a note component that takes an eventid as an argument. hypernote-os doesn't support this but it's something we'll def want. let's discuss this as well.
