// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use html6::{loader, reconciler, renderer};
use masonry::core::{ErasedAction, WidgetId, WidgetTag};
use masonry::dpi::LogicalSize;
use masonry::peniko::color::AlphaColor;
use masonry::peniko::Color;
use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, DisabledContentColor, CaretColor, SelectionColor};
use masonry::theme;
use masonry::widgets::{Button, Flex, Label, Portal, TextArea};
use masonry_winit::app::{AppDriver, DriverCtx, EventLoopProxy, MasonryUserEvent, NewWindow, WindowId};
use masonry_winit::winit::window::Window;
use notify::{Watcher, RecursiveMode, Event};
use std::sync::mpsc;
use std::path::Path;

const CONTENT_TAG: WidgetTag<Flex> = WidgetTag::new("content");

struct Driver {
    window_id: WindowId,
    hnmd_path: String,
    widget_states: Vec<reconciler::WidgetState>,
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
                Ok(doc) => {
                    print_ast(&doc);

                    // Reconcile old and new AST
                    let (new_states, ops) = reconciler::reconcile_nodes(
                        &self.widget_states,
                        &doc.body,
                        "",
                    );

                    // Count operations for reporting
                    let keeps = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Keep)).count();
                    let rebuilds = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Rebuild)).count();
                    let adds = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Add)).count();
                    let removes = ops.iter().filter(|op| matches!(op, reconciler::ReconcileOp::Remove)).count();

                    println!("  📊 Reconciliation: {} kept, {} rebuilt, {} added, {} removed",
                        keeps, rebuilds, adds, removes);

                    // For now, just do a full rebuild (keyed reconciliation implementation coming)
                    // TODO: Use ops to do incremental updates
                    let new_content = renderer::build_document_widget(&doc.body);

                    // Replace the content in the Portal
                    let render_root = ctx.render_root(window_id);
                    render_root.edit_widget_with_tag(CONTENT_TAG, |mut content_flex| {
                        // Clear existing children
                        Flex::clear(&mut content_flex);

                        // Add new widget tree
                        Flex::add_child(&mut content_flex, new_content);
                    });

                    // Update stored states
                    self.widget_states = new_states;

                    println!("✅ UI reloaded successfully!\n");
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
        Node::Spacer { .. } => "Spacer".to_string(),
    }
}

fn main() {
    const HNMD_FILE: &str = "apps/hello.hnmd";

    // Load and parse .hnmd file
    let doc = loader::load_hnmd(HNMD_FILE)
        .expect("Failed to load hello.hnmd");

    // Print AST on startup
    print_ast(&doc);

    // Build initial widget states for reconciliation
    let initial_states = reconciler::build_widget_tree(&doc.body, "");

    // Build widget tree from AST
    // Wrap in tagged Flex so we can update it later
    let content = renderer::build_document_widget(&doc.body);
    let content_flex = masonry::core::NewWidget::new_with_tag(
        Flex::column().with_child(content),
        CONTENT_TAG,
    );

    // Wrap in Portal for scrolling
    let root_widget = masonry::core::NewWidget::new(Portal::new(content_flex));

    // Create window
    let window_size = LogicalSize::new(600.0, 800.0);
    let window_attributes = Window::default_attributes()
        .with_title("HNMD Viewer")
        .with_resizable(true)
        .with_min_inner_size(window_size);

    let driver = Driver {
        window_id: WindowId::next(),
        hnmd_path: HNMD_FILE.to_string(),
        widget_states: initial_states,
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
    let hnmd_path = HNMD_FILE.to_string();
    let (file_tx, file_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if event.kind.is_modify() {
                let _ = file_tx.send(());
            }
        }
    }).expect("Failed to create file watcher");

    watcher.watch(Path::new(HNMD_FILE), RecursiveMode::NonRecursive)
        .expect("Failed to watch file");

    // Spawn thread to watch for file changes and send reload actions
    let window_id = driver.window_id;
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
                let _ = proxy.send_event(user_event);
            }
        }
    });

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
