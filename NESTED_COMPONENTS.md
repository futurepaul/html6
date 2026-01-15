# Nested Components Plan

## Goal

Enable component-scoped queries so each component fetches only the data it needs, avoiding complex root-level joins.

## Architecture: Local Components with Shared Loaders

### File Types

- **`.hnmd`** - Documents (apps) with imports and layout
- **`.hnmc`** - Components with scoped queries and props

### Key Principle

Components define their own queries and receive data via props. All components share global `AddressLoader` and `EventLoader` for automatic deduplication.

---

## Example: Feed with Profiles

### `apps/feed.hnmd` (Document)

```yaml
---
imports:
  Feed: "./components/Feed.hnmc"
---

<Feed limit={50} />
```

### `components/Feed.hnmc` (Component)

```yaml
---
imports:
  Profile: "./Profile.hnmc"

queries:
  notes:
    kinds: [1]
    limit: "{props.limit // 20}"

props:
  limit: number
---

<each from={queries.notes} as="note">
<hstack>

<Profile pubkey={note.pubkey} />

<vstack>

{note.content}

---

</vstack>

</hstack>
</each>
```

### `components/Profile.hnmc` (Component)

```yaml
---
queries:
  metadata:
    kinds: [0]
    authors: ["{props.pubkey}"]
    limit: 1

props:
  pubkey: string
---

<vstack>

**{queries.metadata[0].content | fromjson | .name // "Anonymous"}**

</vstack>
```

---

## How It Works

```
feed.hnmd
  → <Feed limit={50}>
    → queries.notes (50 notes from Nostr)
    → <each> over notes
      → <Profile pubkey={note.pubkey}>
        → queries.metadata for THIS pubkey
        → Uses shared AddressLoader (deduplicated!)
        → Renders name
```

**Key:** Each `<Profile>` executes its own query, but AddressLoader ensures we only fetch each profile once.

---

## Implementation Steps

### Step 1: Extend AST

```rust
// src/parser/ast.rs

pub struct Document {
    pub imports: HashMap<String, String>,  // name → file path
    pub frontmatter: Frontmatter,
    pub body: Vec<Node>,
}

pub enum Node {
    // ... existing nodes
    CustomComponent {
        name: String,
        props: HashMap<String, PropValue>,
        children: Vec<Node>,
    },
}

pub enum PropValue {
    Literal(String),
    Expression(String),  // {note.pubkey}
}
```

**Tasks:**
- [ ] Add `imports` field to Document
- [ ] Parse `imports:` section in frontmatter
- [ ] Add CustomComponent node variant
- [ ] Parse component props with expressions

---

### Step 2: Component Definition Parser

```rust
// src/parser/component_def.rs

pub struct ComponentDef {
    pub imports: HashMap<String, String>,
    pub queries: HashMap<String, ast::Filter>,
    pub props_schema: HashMap<String, PropSchema>,
    pub body: Vec<Node>,
}

pub struct PropSchema {
    pub type_name: String,      // "string", "number"
    pub required: bool,
    pub default: Option<String>,
}

pub fn parse_component(content: &str) -> Result<ComponentDef> {
    // Similar to parse_hnmd but with props section
}
```

**Tasks:**
- [ ] Create ComponentDef struct
- [ ] Parse .hnmc files (frontmatter + body)
- [ ] Parse `props:` section
- [ ] Parse component-scoped `queries:` section

---

### Step 3: Component Registry

```rust
// src/runtime/components.rs

pub struct ComponentRegistry {
    components: HashMap<String, ComponentDef>,
    event_loader: Arc<EventLoader>,
    address_loader: Arc<AddressLoader>,
}

impl ComponentRegistry {
    pub fn load_component(&mut self, name: &str, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let component_def = parse_component(&content)?;
        self.components.insert(name.to_string(), component_def);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ComponentDef> {
        self.components.get(name)
    }
}
```

**Tasks:**
- [ ] Create ComponentRegistry
- [ ] Load components from file paths
- [ ] Store shared loader references

---

### Step 4: Component Rendering

```rust
// src/renderer/components.rs

pub async fn render_component(
    name: &str,
    props: &HashMap<String, PropValue>,
    registry: &ComponentRegistry,
    parent_ctx: &RenderContext,
) -> Result<NewWidget<Flex>> {
    let component_def = registry.get(name)?;

    // 1. Evaluate props in parent context
    let mut prop_values = HashMap::new();
    for (prop_name, prop_val) in props {
        let value = match prop_val {
            PropValue::Literal(s) => json!(s),
            PropValue::Expression(expr) => parent_ctx.runtime_ctx.eval(expr, &mut parent_ctx.evaluator)?,
        };
        prop_values.insert(prop_name.clone(), value);
    }

    // 2. Execute component queries with prop substitution
    let mut component_queries = HashMap::new();
    for (query_id, filter_def) in &component_def.queries {
        // Substitute props in filter (e.g., authors: ["{props.pubkey}"])
        let filter = compile_filter_with_props(filter_def, &prop_values)?;

        // Execute query using shared loaders
        let events = registry.address_loader.load_by_filter(filter).await?;

        component_queries.insert(query_id.clone(), events);
    }

    // 3. Create component-scoped context
    let mut component_ctx = RuntimeContext::new();
    component_ctx.locals.insert("props".to_string(), json!(prop_values));
    component_ctx.queries = queries_to_json(&component_queries);

    // 4. Render component body
    let render_ctx = RenderContext::new(component_ctx);
    build_widget_tree(&component_def.body, Some(render_ctx))
}

fn compile_filter_with_props(
    filter_def: &ast::Filter,
    props: &HashMap<String, Value>,
) -> Result<Filter> {
    // Replace "{props.pubkey}" with actual prop value
    // ...
}
```

**Tasks:**
- [ ] Implement render_component
- [ ] Evaluate props in parent context
- [ ] Substitute props into filter definitions
- [ ] Execute component queries with shared loaders
- [ ] Create scoped RuntimeContext
- [ ] Render component body

---

### Step 5: Integrate into Widget Builder

```rust
// In src/renderer/widgets.rs

Node::CustomComponent { name, props, .. } => {
    // Render component (async!)
    // Need to handle async in widget builder
    // For now, might need to pre-render components before building widgets
}
```

**Challenge:** Widget builder is sync, but component queries are async.

**Solution:** Pre-execute all component queries during document load, then render synchronously.

---

### Step 6: Wire into Main

```rust
// In main.rs

async fn load_app(path: &str) -> Result<(Document, ComponentRegistry)> {
    // 1. Parse document
    let doc = loader::load_hnmd(path)?;

    // 2. Create component registry with shared loaders
    let mut registry = ComponentRegistry::new(address_loader, event_loader);

    // 3. Load all imported components
    for (name, import_path) in &doc.imports {
        registry.load_component(name, import_path)?;
    }

    // 4. Pre-execute component queries
    // (Walk AST, find CustomComponent nodes, execute their queries)

    Ok((doc, registry))
}
```

**Tasks:**
- [ ] Create ComponentRegistry during app init
- [ ] Load component files
- [ ] Pre-execute component queries
- [ ] Pass registry to renderer

---

## Data Flow

```
1. Parse feed.hnmd
   → Find imports: {Feed: "./Feed.hnmc"}
   → Load Feed.hnmc, parse it
   → Find nested imports: {Profile: "./Profile.hnmc"}
   → Load Profile.hnmc, parse it

2. Render feed.hnmd
   → Encounter <Feed limit={50}>
   → Evaluate prop: limit=50
   → Execute Feed's queries: notes (kinds: [1], limit: 50)
   → Render Feed body with queries.notes available

3. Feed renders <each> over notes
   → For each note, encounter <Profile pubkey={note.pubkey}>
   → Evaluate prop: pubkey="abc123..."
   → Execute Profile's query: metadata (kinds: [0], authors: ["abc123"])
   → AddressLoader deduplicates (caches by "0:abc123:")
   → Render Profile body with queries.metadata available
```

---

## Deduplication Example

```
Feed has 50 notes
  → 30 unique authors
  → 100 total <Profile> instances (some authors post multiple times)

AddressLoader:
  → Receives 100 requests for profiles
  → Deduplicates to 30 unique pubkeys
  → Makes 30 Nostr queries (or fewer if batched)
  → Caches results
  → All 100 Profile components get data from cache
```

---

## Implementation Checklist

**Phase 1: Component Loading**
- [ ] Add `imports` to Document AST
- [ ] Create ComponentDef struct and parser
- [ ] Create ComponentRegistry
- [ ] Load .hnmc files from disk

**Phase 2: Prop System**
- [ ] Add CustomComponent node to AST
- [ ] Parse component props
- [ ] Evaluate props in parent context
- [ ] Pass prop values to component

**Phase 3: Component Queries**
- [ ] Parse component-scoped queries
- [ ] Substitute props into query filters
- [ ] Execute queries with shared loaders
- [ ] Create component-scoped RuntimeContext

**Phase 4: Component Rendering**
- [ ] Render CustomComponent nodes
- [ ] Handle async query execution
- [ ] Integrate with existing widget builder
- [ ] Test Profile component

**Phase 5: Full Integration**
- [ ] Test Feed with nested Profile components
- [ ] Verify deduplication works
- [ ] Test live updates
- [ ] Add error handling for missing components/props

---

## Success Criteria

- [ ] Can define Profile.hnmc with scoped metadata query
- [ ] Can import and render `<Profile pubkey={note.pubkey} />`
- [ ] Multiple Profile instances deduplicate correctly
- [ ] Profile names display in feed
- [ ] Adding new components doesn't require root-level changes
- [ ] All existing tests still pass

---

## Future: Nostr Imports

After local components work, add network imports:

```yaml
imports:
  Profile: "naddr1qq...xyz"  # Component published on Nostr
```

Components become:
- Publishable to Nostr
- Versionable and immutable
- Shareable across apps
- Decentralized component library
