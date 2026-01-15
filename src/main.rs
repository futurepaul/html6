// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use html6::{loader, reconciler, renderer, runtime::RuntimeContext, runtime::query::QueryRuntime, runtime::ComponentRegistry};
use masonry::core::{ErasedAction, WidgetId, WidgetTag};
use masonry::dpi::LogicalSize;
use masonry::peniko::color::AlphaColor;
use masonry::peniko::Color;
use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, DisabledContentColor, CaretColor, SelectionColor};
use masonry::theme;
use masonry::widgets::{Button, Flex, Label, Portal, TextArea};
use masonry_winit::app::{AppDriver, DriverCtx, MasonryUserEvent, NewWindow, WindowId};
use masonry_winit::winit::window::Window;
use notify::{Watcher, RecursiveMode, Event};
use std::sync::{mpsc, Arc};
use std::path::Path;

const CONTENT_TAG: WidgetTag<Flex> = WidgetTag::new("content");

struct Driver {
    window_id: WindowId,
    hnmd_path: String,
    widget_states: Vec<reconciler::WidgetState>,
    render_ctx: Option<renderer::RenderContext>,
    query_runtime: Option<Arc<QueryRuntime>>,
    registry: ComponentRegistry,
}

// Custom action to trigger reload
#[derive(Debug)]
struct ReloadAction;

impl AppDriver for Driver {
    fn on_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_, '_>,
        _widget_id: WidgetId,
        action: ErasedAction,
    ) {
        debug_assert_eq!(window_id, self.window_id, "unknown window");

        // Check if this is a reload action
        if action.is::<ReloadAction>() {
            println!("🔄 Reloading UI...");

            // Reload and parse file
            match loader::load_hnmd(&self.hnmd_path) {
                Ok((doc, _registry)) => {
                    print_ast(&doc);

                    // Create runtime context from frontmatter state
                    let mut runtime_ctx = RuntimeContext::with_state(doc.frontmatter.state.clone());

                    // Update with latest query data if available
                    if let Some(qr) = &self.query_runtime {
                        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                        let queries_json = rt.block_on(async {
                            qr.to_json().await
                        });

                        // Debug: Show what we're loading
                        if let Some(obj) = queries_json.as_object() {
                            for (key, value) in obj {
                                if let Some(arr) = value.as_array() {
                                    println!("  📊 Reloading with {}: {} events", key, arr.len());
                                }
                            }
                        }

                        runtime_ctx.queries = queries_json;
                    }

                    let render_ctx = renderer::RenderContext::new(runtime_ctx)
                        .with_registry(self.registry.clone());

                    // Debug: print state changes
                    println!("  🔍 New state: {:?}", doc.frontmatter.state);

                    self.render_ctx = Some(render_ctx.clone());

                    // Reconcile old and new AST with context for expr hash tracking
                    let arena = reconciler::WidgetArena {
                        states: self.widget_states.clone(),
                        generations: vec![0; self.widget_states.len().max(doc.body.len())],
                    };
                    let mut reconcile_ctx = self.render_ctx.clone();
                    let (new_arena, ops) = reconciler::reconcile_arena(
                        &arena,
                        &doc.body,
                        &mut reconcile_ctx,
                    );
                    let new_states = new_arena.states;

                    // Count operations for reporting
                    let keeps = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Keep)).count();
                    let rebuilds = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Rebuild)).count();
                    let adds = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Add)).count();
                    let removes = self.widget_states.len().saturating_sub(doc.body.len());

                    println!("  📊 Reconciliation: {} kept, {} rebuilt, {} added, {} removed",
                        keeps, rebuilds, adds, removes);
                    println!("  📐 Ops: {} ops for {} nodes (old had {} nodes)",
                        ops.len(), doc.body.len(), self.widget_states.len());

                    // Apply updates using positional reconciliation
                    let render_root = ctx.render_root(window_id);
                    render_root.edit_widget_with_tag(CONTENT_TAG, |mut content_flex| {
                        // Process each position
                        for (idx, (node, op)) in doc.body.iter().zip(ops.iter()).enumerate() {
                            match op {
                                reconciler::ReconcileOp::Keep => {
                                    // Widget at this position unchanged - leave it alone
                                }
                                reconciler::ReconcileOp::Rebuild => {
                                    // Widget at this position changed - replace it
                                    Flex::remove_child(&mut content_flex, idx);
                                    Flex::insert_child(&mut content_flex, idx, renderer::build_widget_with_context(node, self.render_ctx.clone()));
                                }
                                reconciler::ReconcileOp::Add => {
                                    // New position (list grew) - append
                                    Flex::add_child(&mut content_flex, renderer::build_widget_with_context(node, self.render_ctx.clone()));
                                }
                                reconciler::ReconcileOp::Remove => {
                                    // Not used in generational approach
                                }
                            }
                        }

                        // Shrink if list got smaller
                        while content_flex.widget.len() > doc.body.len() {
                            Flex::remove_child(&mut content_flex, doc.body.len());
                        }
                    });

                    // Update stored states
                    self.widget_states = new_states;

                    println!("✅ UI updated incrementally!\n");
                }
                Err(e) => {
                    eprintln!("❌ Failed to reload: {}\n", e);
                }
            }
        }
    }
}

fn print_ast(doc: &html6::parser::ast::Document) {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                        AST UPDATED                           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("📄 Document Structure:");
    println!("  Filters: {}", doc.frontmatter.filters.len());
    println!("  Pipes: {}", doc.frontmatter.pipes.len());
    println!("  Actions: {}", doc.frontmatter.actions.len());
    println!("  State: {}", doc.frontmatter.state.len());
    println!("  Body nodes: {}\n", doc.body.len());

    println!("🌳 Body AST:");
    for (i, node) in doc.body.iter().enumerate() {
        println!("  [{}] {:?}", i, node_type(node));
    }

    println!("\n📋 Full AST:");
    for (i, node) in doc.body.iter().enumerate() {
        println!("  {}: {:#?}", i, node);
    }
    println!("\n{}\n", "─".repeat(64));
}

fn node_type(node: &html6::parser::ast::Node) -> String {
    use html6::parser::ast::Node;
    match node {
        Node::Heading { level, .. } => format!("Heading({})", level),
        Node::Paragraph { .. } => "Paragraph".to_string(),
        Node::Text { value } => format!("Text('{}')", value.chars().take(20).collect::<String>()),
        Node::Strong { .. } => "Strong".to_string(),
        Node::Emphasis { .. } => "Emphasis".to_string(),
        Node::List { ordered, .. } => if *ordered { "OrderedList".to_string() } else { "UnorderedList".to_string() },
        Node::Link { url, .. } => format!("Link({})", url),
        Node::Image { src, .. } => format!("Image({})", src),
        Node::Expr { expression } => format!("Expr({})", expression),
        Node::Each { from, as_name, .. } => format!("Each({} as {})", from, as_name),
        Node::If { value, .. } => format!("If({})", value),
        Node::Button { .. } => "Button".to_string(),
        Node::Input { name, .. } => format!("Input({})", name),
        Node::VStack { children, .. } => format!("VStack({} children)", children.len()),
        Node::HStack { children, .. } => format!("HStack({} children)", children.len()),
        Node::Grid { .. } => "Grid".to_string(),
        Node::Json { value } => format!("Json({})", value),
        Node::Spacer { .. } => "Spacer".to_string(),
        Node::CustomComponent { name, .. } => format!("CustomComponent({})", name),
    }
}

fn main() {
    // Get file path from command line args or use default
    let args: Vec<String> = std::env::args().collect();
    let hnmd_file = if args.len() > 1 {
        args[1].as_str()
    } else {
        "apps/hello.hnmd"
    };

    println!("📂 Loading: {}\n", hnmd_file);

    // Load and parse .hnmd file
    let (doc, registry) = loader::load_hnmd(hnmd_file)
        .expect(&format!("Failed to load {}", hnmd_file));

    // Print AST on startup
    print_ast(&doc);

    // Create runtime context from frontmatter state
    let runtime_ctx = RuntimeContext::with_state(doc.frontmatter.state.clone());

    // Initialize QueryRuntime if there are filters
    let query_runtime = if !doc.frontmatter.filters.is_empty() {
        println!("🔌 Initializing Nostr client...");

        // Create tokio runtime for async operations
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let query_runtime = rt.block_on(async {
            // Create QueryRuntime
            let mut qr = QueryRuntime::new().await.expect("Failed to create QueryRuntime");

            // Add relay
            qr.add_relay("wss://relay.damus.io").await.expect("Failed to add relay");
            println!("  ✓ Connected to wss://relay.damus.io");

            // Subscribe to all filters in frontmatter
            for (id, filter_def) in &doc.frontmatter.filters {
                println!("  📡 Subscribing to filter '{}'...", id);
                let _ = qr.subscribe_ast_filter(id, filter_def, &runtime_ctx).await
                    .expect(&format!("Failed to subscribe to filter '{}'", id));
            }

            Arc::new(qr)
        });

        // Spawn tokio runtime in background thread to keep it alive
        std::thread::spawn(move || {
            rt.block_on(async {
                // Keep runtime alive
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            });
        });

        Some(query_runtime)
    } else {
        None
    };

    // Start with empty query results - they'll be populated as events arrive
    let runtime_ctx_with_queries = runtime_ctx.clone();
    println!("  ⚡ Starting UI with empty queries (will update as events arrive)...");

    let render_ctx = renderer::RenderContext::new(runtime_ctx_with_queries)
        .with_registry(registry.clone());

    // Build initial widget states for reconciliation with context
    let mut initial_ctx = Some(render_ctx.clone());
    let initial_states = reconciler::build_widget_tree(&doc.body, &mut initial_ctx);

    // Build widget tree from AST with tag for updates and context
    let content = renderer::build_document_widget_with_context(&doc.body, Some(render_ctx.clone()), Some(CONTENT_TAG));

    // Wrap in Portal for scrolling
    let root_widget = masonry::core::NewWidget::new(Portal::new(content));

    // Create window
    let window_size = LogicalSize::new(600.0, 800.0);
    let window_attributes = Window::default_attributes()
        .with_title("HNMD Viewer")
        .with_resizable(true)
        .with_min_inner_size(window_size);

    // Clone query_runtime for the background thread before moving it to Driver
    let query_runtime_for_bg = query_runtime.clone();

    let driver = Driver {
        window_id: WindowId::next(),
        hnmd_path: hnmd_file.to_string(),
        widget_states: initial_states,
        render_ctx: Some(render_ctx),
        query_runtime,
        registry,
    };

    // Create custom theme with black text on light gray background
    let mut properties = theme::default_property_set();
    properties.insert::<Label, _>(ContentColor::new(Color::from_rgb8(0, 0, 0)));
    properties.insert::<Label, _>(DisabledContentColor(ContentColor::new(Color::from_rgb8(100, 100, 100))));

    // Style buttons with border and darker gray background
    properties.insert::<Button, _>(Background::Color(Color::from_rgb8(192, 192, 192)));
    properties.insert::<Button, _>(BorderColor { color: Color::from_rgb8(128, 128, 128) });
    properties.insert::<Button, _>(BorderWidth { width: 1.0 });

    // Style text inputs with black text and cursor
    properties.insert::<TextArea<true>, _>(ContentColor::new(Color::from_rgb8(0, 0, 0)));
    properties.insert::<TextArea<false>, _>(ContentColor::new(Color::from_rgb8(0, 0, 0)));
    properties.insert::<TextArea<true>, _>(CaretColor { color: AlphaColor::from_rgb8(0, 0, 0) });
    properties.insert::<TextArea<false>, _>(CaretColor { color: AlphaColor::from_rgb8(0, 0, 0) });

    // Style selection to be blue with good contrast
    properties.insert::<TextArea<true>, _>(SelectionColor { color: AlphaColor::from_rgb8(173, 214, 255) });
    properties.insert::<TextArea<false>, _>(SelectionColor { color: AlphaColor::from_rgb8(200, 200, 200) });

    // Create event loop
    let event_loop = masonry_winit::app::EventLoop::with_user_event()
        .build()
        .unwrap();

    // Get proxy for sending events from other threads
    let proxy = event_loop.create_proxy();

    // Set up file watcher with proxy
    let watch_path = hnmd_file.to_string();
    let (file_tx, file_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if event.kind.is_modify() {
                let _ = file_tx.send(());
            }
        }
    }).expect("Failed to create file watcher");

    watcher.watch(Path::new(&watch_path), RecursiveMode::NonRecursive)
        .expect("Failed to watch file");

    // Spawn thread to watch for file changes and send reload actions
    let window_id = driver.window_id;
    let proxy_clone = proxy.clone();
    std::thread::spawn(move || {
        // Keep watcher alive
        let _watcher = watcher;

        loop {
            if file_rx.recv().is_ok() {
                println!("\n📝 File changed detected...");

                // Send reload action via event loop proxy
                let action: masonry::core::ErasedAction = Box::new(ReloadAction);
                let widget_id = WidgetId::next();
                let user_event = MasonryUserEvent::Action(window_id, action, widget_id);
                let _ = proxy_clone.send_event(user_event);
            }
        }
    });

    // Spawn thread to watch for query updates and trigger UI refreshes
    if let Some(qr) = query_runtime_for_bg.as_ref() {
        let qr_clone = Arc::clone(qr);
        let window_id = driver.window_id;

        std::thread::spawn(move || {
            // Poll for query updates every 500ms
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));

                // Check if we have any queries with events
                let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                let has_events = rt.block_on(async {
                    let queries_json = qr_clone.to_json().await;
                    if let Some(obj) = queries_json.as_object() {
                        obj.values().any(|v| v.as_array().map(|a| !a.is_empty()).unwrap_or(false))
                    } else {
                        false
                    }
                });

                if has_events {
                    println!("  🔄 Query updated, refreshing UI...");

                    // Send reload action to update UI
                    let action: masonry::core::ErasedAction = Box::new(ReloadAction);
                    let widget_id = WidgetId::next();
                    let user_event = MasonryUserEvent::Action(window_id, action, widget_id);
                    let _ = proxy.send_event(user_event);

                    // Only refresh once when events first arrive
                    break;
                }
            }
        });
    }

    // Run app
    masonry_winit::app::run_with(
        event_loop,
        vec![NewWindow::new_with_id(
            driver.window_id,
            window_attributes,
            root_widget.erased(),
        )
        .with_base_color(AlphaColor::from_rgb8(236, 236, 236))], // Classic light gray
        driver,
        properties,
    )
    .unwrap();
}
