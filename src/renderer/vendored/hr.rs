// Copyright 2025 HNMD Authors
// SPDX-License-Identifier: Apache-2.0
//
// Custom HR widget for horizontal rules / separators

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, Property, RegisterCtx, Update, UpdateCtx, Widget,
    WidgetId, HasProperty,
};
use masonry::peniko::Color;
use masonry::properties::Padding;
use masonry::vello::kurbo::{Line, Size};
use masonry::vello::Scene;
use tracing::{Span, trace_span};

/// A horizontal rule / separator widget
pub struct Hr {
    /// Height of the spacer around the line
    height: f64,
}

impl Hr {
    pub fn new() -> Self {
        Self { height: 20.0 }
    }

    pub fn with_height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }
}

impl Default for Hr {
    fn default() -> Self {
        Self::new()
    }
}

// Custom property for HR color
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HrColor {
    pub color: Color,
}

impl Property for HrColor {
    fn static_default() -> &'static Self {
        static DEFAULT: HrColor = HrColor {
            color: Color::from_rgba8(128, 128, 128, 255),
        };
        &DEFAULT
    }
}

impl Default for HrColor {
    fn default() -> Self {
        *Self::static_default()
    }
}

impl HrColor {
    pub const fn new(color: Color) -> Self {
        Self { color }
    }
}

impl HasProperty<HrColor> for Hr {}
impl HasProperty<Padding> for Hr {}

impl Widget for Hr {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _event: &Update) {}

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let padding = props.get::<Padding>();
        let padding_size = padding.top + padding.bottom;

        // Use full width, height is spacer height + padding
        Size::new(bc.max().width, self.height + padding_size)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let color = props.get::<HrColor>().color;
        let padding = props.get::<Padding>();

        // Draw line in the vertical center
        let y = size.height / 2.0;
        let line = Line::new((padding.left, y), (size.width - padding.right, y));

        let brush = color;
        masonry::util::stroke(scene, &line, brush, 1.0);
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::default()
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("Hr", id = id.trace())
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("height={}", self.height))
    }
}
