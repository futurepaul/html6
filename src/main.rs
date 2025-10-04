// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use html6::{loader, renderer};
use masonry::core::ErasedAction;
use masonry::core::WidgetId;
use masonry::dpi::LogicalSize;
use masonry::peniko::color::AlphaColor;
use masonry::peniko::Color;
use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, DisabledContentColor, CaretColor, SelectionColor};
use masonry::theme;
use masonry::widgets::{Button, Label, Portal, TextArea};
use masonry_winit::app::{AppDriver, DriverCtx, NewWindow, WindowId};
use masonry_winit::winit::window::Window;

struct Driver {
    window_id: WindowId,
}

impl AppDriver for Driver {
    fn on_action(
        &mut self,
        window_id: WindowId,
        _ctx: &mut DriverCtx<'_, '_>,
        _widget_id: WidgetId,
        _action: ErasedAction,
    ) {
        debug_assert_eq!(window_id, self.window_id, "unknown window");
    }
}

fn main() {
    // Load and parse .hnmd file
    let doc = loader::load_hnmd("apps/hello.hnmd")
        .expect("Failed to load hello.hnmd");

    // Debug: Print AST (commented out for production)
    // println!("=== Parsed AST ===");
    // for (i, node) in doc.body.iter().enumerate() {
    //     println!("{}: {:#?}", i, node);
    // }
    // println!("==================\n");

    // Build widget tree from AST and wrap in Portal for scrolling
    let content = renderer::build_document_widget(&doc.body);
    let root_widget = masonry::core::NewWidget::new(Portal::new(content));

    // Create window
    let window_size = LogicalSize::new(600.0, 800.0);
    let window_attributes = Window::default_attributes()
        .with_title("HNMD Viewer")
        .with_resizable(true)
        .with_min_inner_size(window_size);

    let driver = Driver {
        window_id: WindowId::next(),
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

    // Run app
    let event_loop = masonry_winit::app::EventLoop::with_user_event()
        .build()
        .unwrap();
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
