use crate::parser::ast::Node;
use crate::renderer::vendored::{TextInput, FocusedBorderColor, Hr, HrColor};
use crate::runtime::{ComponentRegistry, JaqEvaluator, RuntimeContext};
use masonry::core::{NewWidget, Properties, StyleProperty};
use masonry::parley::style::{FontStyle, FontWeight};
use masonry::peniko::Color;
use masonry::peniko::color::AlphaColor;
use masonry::properties::{Background, BorderColor, BorderWidth, CornerRadius, ObjectFit, Padding, CaretColor, SelectionColor, UnfocusedSelectionColor};
use masonry::properties::types::{CrossAxisAlignment, Length, MainAxisAlignment};
use masonry::widgets::{Button, Flex, Image, Label};
use serde_json::{json, Value};
use std::fs;

/// Debug flag to show borders around layout containers
const DEBUG_LAYOUT: bool = false;

/// Context for rendering widgets with runtime data
#[derive(Clone)]
pub struct RenderContext {
    pub runtime_ctx: RuntimeContext,
    pub evaluator: JaqEvaluator,
    pub registry: Option<ComponentRegistry>,
}

impl RenderContext {
    pub fn new(runtime_ctx: RuntimeContext) -> Self {
        Self {
            runtime_ctx,
            evaluator: JaqEvaluator::new(),
            registry: None,
        }
    }

    pub fn with_registry(mut self, registry: ComponentRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Evaluate an expression using this context
    pub fn eval(&mut self, expression: &str) -> Result<Value, String> {
        self.runtime_ctx
            .eval(expression, &mut self.evaluator)
            .map_err(|e| e.to_string())
    }
}

/// Wrap a widget in a Flex column for consistent typing
fn wrap_in_flex<W: masonry::core::Widget + 'static>(widget: NewWidget<W>) -> NewWidget<Flex> {
    NewWidget::new(Flex::column().with_child(widget))
}

/// Extract flex value from a node if it has one
fn get_child_flex(node: &Node) -> Option<f64> {
    match node {
        Node::VStack { flex, .. } | Node::HStack { flex, .. } => *flex,
        _ => None,
    }
}

/// Convert a JSON value to a displayable string
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string_pretty(value).unwrap_or_default(),
    }
}

/// Build an inline widget (for use in paragraphs/lists with mixed formatting)
/// The `add_space` parameter adds a trailing space for word separation
fn build_inline_widget(node: &Node, add_space: bool) -> NewWidget<Label> {
    let props = Properties::new().with(Padding::ZERO);
    let space = if add_space { " " } else { "" };

    match node {
        Node::Text { value } => NewWidget::new_with_props(
            Label::new(format!("{}{}", value, space)).with_style(StyleProperty::FontSize(18.0)),
            props,
        ),
        Node::Strong { children } => {
            let text = render_children_to_text(children);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space))
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontWeight(FontWeight::BOLD)),
                props,
            )
        }
        Node::Emphasis { children } => {
            let text = render_children_to_text(children);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space))
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontStyle(FontStyle::Italic)),
                props,
            )
        }
        Node::Link { children, .. } => {
            let text = render_children_to_text(children);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space)).with_style(StyleProperty::FontSize(18.0)),
                props,
            )
        }
        _ => NewWidget::new_with_props(
            Label::new(format!("{}{}", render_children_to_text(&[node.clone()]), space))
                .with_style(StyleProperty::FontSize(18.0)),
            props,
        ),
    }
}

fn build_inline_widget_with_context(node: &Node, add_space: bool, ctx: &mut Option<RenderContext>) -> NewWidget<Label> {
    let props = Properties::new().with(Padding::ZERO);
    let space = if add_space { " " } else { "" };

    match node {
        Node::Text { value } => NewWidget::new_with_props(
            Label::new(format!("{}{}", value, space)).with_style(StyleProperty::FontSize(18.0)),
            props,
        ),
        Node::Strong { children } => {
            let text = render_children_to_text_with_context(children, ctx);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space))
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontWeight(FontWeight::BOLD)),
                props,
            )
        }
        Node::Emphasis { children } => {
            let text = render_children_to_text_with_context(children, ctx);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space))
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontStyle(FontStyle::Italic)),
                props,
            )
        }
        Node::Link { children, .. } => {
            let text = render_children_to_text_with_context(children, ctx);
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space)).with_style(StyleProperty::FontSize(18.0)),
                props,
            )
        }
        Node::Expr { expression } => {
            let text = if let Some(context) = ctx {
                match context.eval(expression) {
                    Ok(value) => value_to_string(&value),
                    Err(_) => format!("{{{}}} [error]", expression),
                }
            } else {
                format!("{{{}}}", expression)
            };
            NewWidget::new_with_props(
                Label::new(format!("{}{}", text, space)).with_style(StyleProperty::FontSize(18.0)),
                props,
            )
        }
        _ => NewWidget::new_with_props(
            Label::new(format!("{}{}", node_to_text_with_context(node, ctx), space))
                .with_style(StyleProperty::FontSize(18.0)),
            props,
        ),
    }
}

/// Build a Masonry widget from an AST node (static rendering, no data)
/// Returns a Flex container for consistent typing
pub fn build_widget(node: &Node) -> NewWidget<Flex> {
    build_widget_with_context(node, None)
}

pub fn build_widget_with_context(
    node: &Node,
    ctx: Option<RenderContext>,
) -> NewWidget<Flex> {
    let mut ctx_mut = ctx.clone();

    match node {
        Node::Heading { level, children } => {
            let text = render_children_to_text_with_context(children, &mut ctx_mut);
            let size = match level {
                1 => 40.0,
                2 => 30.0,
                3 => 24.0,
                4 => 20.0,
                5 => 18.0,
                _ => 16.0,
            };

            wrap_in_flex(NewWidget::new(
                Label::new(text)
                    .with_style(StyleProperty::FontSize(size))
                    .with_style(StyleProperty::FontWeight(FontWeight::BOLD)),
            ))
        }

        Node::Paragraph { children } => {
            // Special case: if paragraph contains only an image, render it directly
            if children.len() == 1 && matches!(&children[0], Node::Image { .. }) {
                return build_widget_with_context(&children[0], ctx.clone());
            }

            // Handle inline formatting by building widgets for each child
            if children.iter().any(|c| matches!(c, Node::Strong { .. } | Node::Emphasis { .. } | Node::Link { .. } | Node::Expr { .. })) {
                let mut flex = Flex::row()
                    .with_gap(Length::ZERO)
                    .main_axis_alignment(MainAxisAlignment::Start);
                let len = children.len();
                for (i, child) in children.iter().enumerate() {
                    let add_space = i < len - 1;
                    flex = flex.with_child(build_inline_widget_with_context(child, add_space, &mut ctx_mut));
                }
                NewWidget::new(flex)
            } else {
                let text = render_children_to_text_with_context(children, &mut ctx_mut);
                wrap_in_flex(NewWidget::new(
                    Label::new(text).with_style(StyleProperty::FontSize(18.0))
                ))
            }
        }

        Node::Text { value } => wrap_in_flex(NewWidget::new(
            Label::new(value.clone()).with_style(StyleProperty::FontSize(18.0))
        )),

        Node::Strong { children } => {
            let text = render_children_to_text_with_context(children, &mut ctx_mut);
            wrap_in_flex(NewWidget::new(
                Label::new(text)
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontWeight(FontWeight::BOLD)),
            ))
        }

        Node::Emphasis { children } => {
            let text = render_children_to_text_with_context(children, &mut ctx_mut);
            wrap_in_flex(NewWidget::new(
                Label::new(text)
                    .with_style(StyleProperty::FontSize(18.0))
                    .with_style(StyleProperty::FontStyle(FontStyle::Italic)),
            ))
        }

        Node::Link { url: _, children } => {
            let text = render_children_to_text_with_context(children, &mut ctx_mut);
            wrap_in_flex(NewWidget::new(Label::new(text)))
        }

        Node::Image { src, alt } => {
            // Try to load image from file
            match load_image(src) {
                Ok(image_brush) => {
                    use masonry::widgets::SizedBox;

                    // Wrap image in SizedBox to constrain its size
                    // This prevents images from expanding beyond their container
                    let image = Image::new(image_brush);
                    let props = Properties::new()
                        .with(ObjectFit::Contain);  // Maintain aspect ratio

                    // Constrain to max 100x100 for avatar-style images
                    // The container's flex will further constrain if needed
                    let sized_image = SizedBox::new(NewWidget::new_with_props(image, props))
                        .width(Length::px(100.0))
                        .height(Length::px(100.0));

                    wrap_in_flex(NewWidget::new(sized_image))
                }
                Err(e) => {
                    // Fallback to showing alt text if image fails to load
                    eprintln!("Failed to load image {}: {}", src, e);
                    wrap_in_flex(NewWidget::new(Label::new(format!("[Image: {}]", alt))))
                }
            }
        }

        Node::List { ordered, items } => {
            let mut flex = Flex::column();
            for (i, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}. ", i + 1)
                } else {
                    "• ".to_string()
                };

                // Build a row with marker + inline content
                let mut row = Flex::row().with_gap(Length::ZERO);
                row = row.with_child(NewWidget::new_with_props(
                    Label::new(marker).with_style(StyleProperty::FontSize(18.0)),
                    Properties::new().with(Padding::ZERO),
                ));

                for child in &item.children {
                    // Check if child has inline formatting
                    if let Node::Paragraph { children } = child {
                        let len = children.len();
                        for (i, inline_child) in children.iter().enumerate() {
                            let add_space = i < len - 1;
                            row = row.with_child(build_inline_widget(inline_child, add_space));
                        }
                    } else {
                        row = row.with_child(build_inline_widget(child, false));
                    }
                }

                flex = flex.with_child(NewWidget::new(row));
            }
            NewWidget::new(flex)
        }

        Node::VStack { children, width, height, flex: flex_val, align } => {
            let mut flex_widget = Flex::column()
                .main_axis_alignment(MainAxisAlignment::Start)
                .cross_axis_alignment(CrossAxisAlignment::Start);

            // Add children - use flex attribute if specified
            for child in children {
                if let Some(child_flex) = get_child_flex(child) {
                    flex_widget = flex_widget.with_flex_child(build_widget_with_context(child, ctx.clone()), child_flex);
                } else {
                    flex_widget = flex_widget.with_child(build_widget_with_context(child, ctx.clone()));
                }
            }

            // Apply width/height constraints if specified
            let mut props = if DEBUG_LAYOUT {
                Properties::new()
                    .with(BorderColor { color: Color::from_rgb8(255, 0, 0) })
                    .with(BorderWidth { width: 1.0 })
                    .with(Padding::from_vh(4.0, 4.0))
            } else {
                Properties::new()
            };

            // TODO: Apply width/height/align properties when Masonry supports them

            NewWidget::new_with_props(flex_widget, props)
        }

        Node::HStack { children, width, height, flex: flex_val, align } => {
            let mut flex_widget = Flex::row()
                .main_axis_alignment(MainAxisAlignment::Start)
                .cross_axis_alignment(CrossAxisAlignment::Start);

            // Add children - use flex attribute if specified
            for child in children {
                if let Some(child_flex) = get_child_flex(child) {
                    flex_widget = flex_widget.with_flex_child(build_widget_with_context(child, ctx.clone()), child_flex);
                } else {
                    flex_widget = flex_widget.with_child(build_widget_with_context(child, ctx.clone()));
                }
            }

            // Apply width/height constraints if specified
            let mut props = if DEBUG_LAYOUT {
                Properties::new()
                    .with(BorderColor { color: Color::from_rgb8(0, 0, 255) })
                    .with(BorderWidth { width: 1.0 })
                    .with(Padding::from_vh(4.0, 4.0))
            } else {
                Properties::new()
            };

            // TODO: Apply width/height/align properties when Masonry supports them

            NewWidget::new_with_props(flex_widget, props)
        }

        Node::Button { on_click: _, children } => {
            let text = render_children_to_text(children);
            let button_props = Properties::new()
                .with(Background::Color(Color::from_rgb8(200, 200, 200)))
                .with(BorderColor { color: Color::from_rgb8(128, 128, 128) })
                .with(BorderWidth { width: 1.0 })
                .with(CornerRadius { radius: 4.0 })
                .with(Padding::from_vh(8., 16.));
            wrap_in_flex(NewWidget::new_with_props(Button::with_text(text), button_props))
        }

        Node::Input { name, placeholder } => {
            // Create TextInput with placeholder
            let mut input = TextInput::new("");
            if let Some(ph) = placeholder {
                input = input.with_placeholder(ph.as_str());
            } else {
                input = input.with_placeholder(name.as_str());
            }

            // Style the input with our light mode theme
            let input_props = Properties::new()
                .with(BorderColor { color: Color::from_rgb8(128, 128, 128) })
                .with(FocusedBorderColor::new(Color::from_rgb8(0, 122, 255)))  // Blue focus border
                .with(BorderWidth { width: 1.0 })
                .with(CornerRadius { radius: 4.0 })
                .with(Padding::from_vh(8., 12.))
                .with(Background::Color(Color::from_rgb8(255, 255, 255)))
                .with(CaretColor { color: AlphaColor::from_rgb8(0, 0, 0) })
                .with(SelectionColor { color: AlphaColor::from_rgb8(173, 214, 255) })
                .with(UnfocusedSelectionColor(SelectionColor { color: AlphaColor::from_rgb8(200, 200, 200) }));
            wrap_in_flex(NewWidget::new_with_props(input, input_props))
        }

        // JSON debug viewer - renders JSON with pretty formatting
        Node::Json { value } => {
            let text = if let Some(mut ctx) = ctx.clone() {
                // Try to evaluate the expression and pretty-print JSON
                match ctx.eval(value) {
                    Ok(json_value) => {
                        // Pretty print the JSON
                        serde_json::to_string_pretty(&json_value)
                            .unwrap_or_else(|_| format!("{{{}}} [json error]", value))
                    }
                    Err(_) => format!("{{{}}} [eval error]", value),
                }
            } else {
                // No context, show placeholder
                format!("<json value={{{}}} />", value)
            };

            // Use a monospace-style label for JSON display
            let json_props = Properties::new()
                .with(Background::Color(Color::from_rgb8(245, 245, 245)))
                .with(Padding::from_vh(12., 12.))
                .with(BorderColor { color: Color::from_rgb8(200, 200, 200) })
                .with(BorderWidth { width: 1.0 })
                .with(CornerRadius { radius: 4.0 });

            wrap_in_flex(NewWidget::new_with_props(Label::new(text), json_props))
        }

        // Expression evaluation - render the evaluated value or placeholder
        Node::Expr { expression } => {
            let text = if let Some(mut ctx) = ctx.clone() {
                // Try to evaluate the expression
                match ctx.eval(expression) {
                    Ok(value) => value_to_string(&value),
                    Err(_) => format!("{{{}}} [error]", expression),
                }
            } else {
                // No context, show placeholder
                format!("{{{}}}", expression)
            };
            wrap_in_flex(NewWidget::new(Label::new(text)))
        }

        Node::Each { from, as_name, children } => {
            // If we have a context, evaluate the `from` expression to get an array
            if let Some(mut render_ctx) = ctx_mut.clone() {
                let items = match render_ctx.runtime_ctx.eval(from, &mut render_ctx.evaluator) {
                    Ok(value) => {
                        if let Value::Array(arr) = value {
                            arr
                        } else {
                            // If it's not an array, treat it as a single-item array
                            vec![value]
                        }
                    }
                    Err(e) => {
                        eprintln!("Error evaluating each expression '{}': {}", from, e);
                        vec![]
                    }
                };

                // Build a vstack containing all the items
                let mut flex = Flex::column();

                for (index, item) in items.iter().enumerate() {
                    // Create scoped context with item binding
                    let scoped_runtime_ctx = render_ctx.runtime_ctx
                        .with_local(as_name, item.clone())
                        .with_local("itemIndex", json!(index));

                    let scoped_ctx = RenderContext {
                        runtime_ctx: scoped_runtime_ctx,
                        evaluator: render_ctx.evaluator.clone(),
                        registry: render_ctx.registry.clone(),
                    };

                    // Render children with scoped context
                    for child in children {
                        flex = flex.with_child(build_widget_with_context(child, Some(scoped_ctx.clone())));
                    }
                }

                NewWidget::new(flex)
            } else {
                // No context available, show placeholder
                wrap_in_flex(NewWidget::new(Label::new(format!(
                    "[Each: {} as {} - no context]",
                    from, as_name
                ))))
            }
        }

        Node::If { value, .. } => wrap_in_flex(NewWidget::new(Label::new(format!("[If: {}]", value)))),

        Node::Grid { children, .. } => {
            let mut flex = Flex::column();
            for child in children {
                flex = flex.with_child(build_widget_with_context(child, ctx.clone()));
            }
            NewWidget::new(flex)
        }

        Node::Spacer { size } => {
            let height = size.unwrap_or(20.0);
            // Use our custom Hr widget for horizontal rules
            let hr = Hr::new().with_height(height);
            let props = Properties::new()
                .with(HrColor::new(Color::from_rgb8(180, 180, 180)))
                .with(Padding::from_vh(8.0, 0.0));
            wrap_in_flex(NewWidget::new_with_props(hr, props))
        }

        Node::CustomComponent { name, props, children } => {
            render_custom_component(name, props, children, &ctx)
        }
    }
}

/// Render a custom component by looking it up in the registry
fn render_custom_component(
    name: &str,
    props: &std::collections::HashMap<String, crate::parser::ast::PropValue>,
    _children: &[Node],
    parent_ctx: &Option<RenderContext>,
) -> NewWidget<Flex> {
    // Get component registry from context
    let ctx = match parent_ctx {
        Some(c) => c,
        None => {
            return wrap_in_flex(NewWidget::new(
                Label::new(format!("[Component: {} (no context)]", name))
            ));
        }
    };

    let registry = match &ctx.registry {
        Some(r) => r,
        None => {
            return wrap_in_flex(NewWidget::new(
                Label::new(format!("[Component: {} (no registry)]", name))
            ));
        }
    };

    // Look up component definition
    let Some(component_def) = registry.get(name) else {
        return wrap_in_flex(NewWidget::new(
            Label::new(format!("[Component: {} not found]", name))
                .with_style(StyleProperty::FontSize(14.0))
        ));
    };

    // Evaluate props in parent context
    let mut prop_values = std::collections::HashMap::new();
    let mut parent_ctx_mut = parent_ctx.clone();
    for (prop_name, prop_val) in props {
        let value = match prop_val {
            crate::parser::ast::PropValue::Literal(s) => json!(s),
            crate::parser::ast::PropValue::Expression(expr) => {
                // Evaluate expression in parent context
                if let Some(ctx) = &mut parent_ctx_mut {
                    ctx.eval(expr).unwrap_or_else(|e| {
                        eprintln!("⚠️  Failed to evaluate prop '{}' expression '{}': {}", prop_name, expr, e);
                        json!(format!("[eval error: {}]", e))
                    })
                } else {
                    json!(expr) // No context, use literal
                }
            }
        };
        prop_values.insert(prop_name.clone(), value);
    }

    // Create component-scoped context with props
    let mut component_runtime_ctx = RuntimeContext::new();
    component_runtime_ctx.locals.insert("props".to_string(), json!(prop_values));

    // Inherit queries from parent context (components share the same query results)
    component_runtime_ctx.queries = ctx.runtime_ctx.queries.clone();

    // Create render context for component
    let mut component_ctx = RenderContext::new(component_runtime_ctx);
    if let Some(ref reg) = ctx.registry {
        component_ctx = component_ctx.with_registry(reg.clone());
    }

    // Render component body
    let mut flex = Flex::column();
    for node in &component_def.body {
        flex = flex.with_child(build_widget_with_context(node, Some(component_ctx.clone())));
    }

    NewWidget::new(flex)
}

/// Render child nodes to text (for labels)
fn render_children_to_text(children: &[Node]) -> String {
    children.iter().map(node_to_text).collect()
}

/// Convert a single node to text
fn node_to_text(node: &Node) -> String {
    match node {
        Node::Text { value } => value.clone(),
        Node::Strong { children } => render_children_to_text(children),
        Node::Emphasis { children } => render_children_to_text(children),
        Node::Link { children, .. } => render_children_to_text(children),
        Node::Paragraph { children } => render_children_to_text(children),
        Node::Expr { expression } => format!("{{{}}}", expression),
        _ => String::new(),
    }
}

/// Convert children to text with expression evaluation
fn render_children_to_text_with_context(children: &[Node], ctx: &mut Option<RenderContext>) -> String {
    children.iter().map(|node| node_to_text_with_context(node, ctx)).collect()
}

/// Convert a single node to text with expression evaluation
fn node_to_text_with_context(node: &Node, ctx: &mut Option<RenderContext>) -> String {
    match node {
        Node::Text { value } => value.clone(),
        Node::Strong { children } => render_children_to_text_with_context(children, ctx),
        Node::Emphasis { children } => render_children_to_text_with_context(children, ctx),
        Node::Link { children, .. } => render_children_to_text_with_context(children, ctx),
        Node::Paragraph { children } => render_children_to_text_with_context(children, ctx),
        Node::Expr { expression } => {
            if let Some(context) = ctx {
                match context.eval(expression) {
                    Ok(value) => value_to_string(&value),
                    Err(e) => {
                        eprintln!("⚠️  Failed to evaluate expression '{}': {}", expression, e);
                        format!("{{{}}} [error]", expression)
                    }
                }
            } else {
                format!("{{{}}}", expression)
            }
        }
        _ => String::new(),
    }
}

/// Build a complete document widget tree
pub fn build_document_widget(nodes: &[Node]) -> NewWidget<Flex> {
    build_document_widget_tagged(nodes, None)
}

pub fn build_document_widget_tagged(nodes: &[Node], tag: Option<masonry::core::WidgetTag<Flex>>) -> NewWidget<Flex> {
    build_document_widget_with_context(nodes, None, tag)
}

pub fn build_document_widget_with_context(
    nodes: &[Node],
    ctx: Option<RenderContext>,
    tag: Option<masonry::core::WidgetTag<Flex>>,
) -> NewWidget<Flex> {
    let mut flex = Flex::column();

    for node in nodes {
        flex = flex.with_child(build_widget_with_context(node, ctx.clone()));
    }

    if let Some(tag) = tag {
        NewWidget::new_with_tag(flex, tag)
    } else {
        NewWidget::new(flex)
    }
}

/// Load an image from a file path and convert to ImageBrush
fn load_image(path: &str) -> Result<masonry::vello::peniko::ImageBrush, Box<dyn std::error::Error>> {
    use masonry::vello::peniko::{ImageBrush, ImageFormat, ImageData, ImageAlphaType, Blob};

    // Read image file
    let bytes = fs::read(path)?;

    // Decode image using image crate
    let img = ::image::load_from_memory(&bytes)?;
    let rgba = img.to_rgba8();

    let (width, height) = rgba.dimensions();
    let data = rgba.into_raw();

    // Create Peniko image
    let peniko_image = ImageData {
        data: Blob::new(std::sync::Arc::new(data)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,  // Unpremultiplied alpha
        width,
        height,
    };

    Ok(ImageBrush::new(peniko_image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::*;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    #[test]
    fn test_render_heading() {
        let node = Node::heading(1, vec![Node::text("Hello World")]);
        let widget = build_widget(&node);

        let harness = TestHarness::create(default_property_set(), widget);
        let root = harness.root_widget();

        // Check that widget was created
        assert!(root.ctx().size().width > 0.0);
    }

    #[test]
    fn test_render_paragraph() {
        let node = Node::paragraph(vec![Node::text("This is a test")]);
        let widget = build_widget(&node);

        let harness = TestHarness::create(default_property_set(), widget);
        let root = harness.root_widget();

        assert!(root.ctx().size().width > 0.0);
    }

    #[test]
    fn test_render_paragraph_with_bold() {
        let node = Node::paragraph(vec![
            Node::text("This is "),
            Node::strong(vec![Node::text("bold")]),
            Node::text(" text"),
        ]);
        let widget = build_widget(&node);

        let harness = TestHarness::create(default_property_set(), widget);
        assert!(harness.root_widget().ctx().size().width > 0.0);
    }

    #[test]
    fn test_render_paragraph_with_italic() {
        let node = Node::paragraph(vec![
            Node::text("This is "),
            Node::emphasis(vec![Node::text("italic")]),
            Node::text(" text"),
        ]);
        let widget = build_widget(&node);

        let harness = TestHarness::create(default_property_set(), widget);
        assert!(harness.root_widget().ctx().size().width > 0.0);
    }

    #[test]
    fn test_render_list_with_formatting() {
        let node = Node::List {
            ordered: false,
            items: vec![
                ListItem {
                    children: vec![Node::paragraph(vec![
                        Node::text("Static markdown rendering"),
                    ])],
                },
                ListItem {
                    children: vec![Node::paragraph(vec![
                        Node::strong(vec![Node::text("Bold")]),
                        Node::text(" and "),
                        Node::emphasis(vec![Node::text("italic")]),
                        Node::text(" text"),
                    ])],
                },
            ],
        };

        let widget = build_widget(&node);
        let harness = TestHarness::create(default_property_set(), widget);
        assert!(harness.root_widget().ctx().size().height > 0.0);
    }

    #[test]
    fn test_render_vstack() {
        let node = Node::vstack(vec![
            Node::heading(1, vec![Node::text("Title")]),
            Node::paragraph(vec![Node::text("Content")]),
        ]);

        let widget = build_widget(&node);
        let harness = TestHarness::create(default_property_set(), widget);

        // Should have rendered both children
        assert!(harness.root_widget().ctx().size().height > 0.0);
    }

    #[test]
    fn test_render_button() {
        let node = Node::button(None, vec![Node::text("Click me")]);
        let widget = build_widget(&node);

        let harness = TestHarness::create(default_property_set(), widget);
        assert!(harness.root_widget().ctx().size().width > 0.0);
    }

    #[test]
    fn test_render_document() {
        let nodes = vec![
            Node::heading(1, vec![Node::text("My App")]),
            Node::paragraph(vec![Node::text("Welcome!")]),
            Node::button(None, vec![Node::text("Start")]),
        ];

        let widget = build_document_widget(&nodes);
        let harness = TestHarness::create(default_property_set(), widget);

        // Document should have height from all children
        let height = harness.root_widget().ctx().size().height;
        assert!(height > 100.0, "Expected combined height, got {}", height);
    }

    #[test]
    fn test_render_list() {
        let node = Node::List {
            ordered: true,
            items: vec![
                ListItem {
                    children: vec![Node::text("First")],
                },
                ListItem {
                    children: vec![Node::text("Second")],
                },
            ],
        };

        let widget = build_widget(&node);
        let harness = TestHarness::create(default_property_set(), widget);

        assert!(harness.root_widget().ctx().size().height > 0.0);
    }
}
