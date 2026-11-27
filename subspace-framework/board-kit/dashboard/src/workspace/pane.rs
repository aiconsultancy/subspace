//! The pane: the work area between the docks. A tab strip names what is
//! open, a toolbar holds the search, filters and actions for it, and the
//! content fills the rest.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    RenderOnce, Stateful, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::{FOCUS_RING_PX, HAIRLINE_PX, strip_height, text_edge};
use crate::icon_button::Metrics;
use crate::tokens::Tokens;

/// The work area: an optional tab strip, an optional [`Toolbar`], then the
/// content, added with `child` or `children`. The content owns its scrolling.
#[derive(IntoElement)]
pub struct Pane {
    id: ElementId,
    tabs: Option<AnyElement>,
    toolbar: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl Pane {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            tabs: None,
            toolbar: None,
            children: Vec::new(),
        }
    }

    /// The tab strip's tabs, usually a `tabs::tab_bar`. It names what is
    /// open in the pane; a Space's views belong in its navigator.
    pub fn tabs(mut self, tabs: impl IntoElement) -> Self {
        self.tabs = Some(tabs.into_any_element());
        self
    }

    pub fn toolbar(mut self, toolbar: impl IntoElement) -> Self {
        self.toolbar = Some(toolbar.into_any_element());
        self
    }
}

impl ParentElement for Pane {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Pane {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .id(self.id)
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(tokens.bg)
            .child(pane_head(self.tabs, self.toolbar, cx))
            .child(
                v_flex()
                    .id("workspace-content")
                    .test_support()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .children(self.children),
            )
    }
}

/// A pane's tab strip and toolbar on their own, for a view that draws its
/// own content column, such as a shared list that keeps its detail panel
/// beside it (`list::render_under`). Either may be left out; a pane beside a
/// dock that has no tab strip makes its toolbar a [`Toolbar::strip`].
pub fn pane_head(tabs: Option<AnyElement>, toolbar: Option<AnyElement>, _cx: &App) -> Div {
    v_flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .children(tabs.map(|tabs| TabStrip { tabs }))
        .children(toolbar)
}

/// The strip is exactly as tall as a dock's header ([`strip_height`]), and
/// shares the docks' surface so the open tab reads as part of the content
/// below it. Its bottom hairline lies behind the tabs, which fill the strip
/// (`tabs::pane_tab_bar`), so the open tab's fill covers it and joins the
/// content.
#[derive(IntoElement)]
struct TabStrip {
    tabs: AnyElement,
}

impl RenderOnce for TabStrip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        h_flex()
            .id("workspace-tab-strip")
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(strip_height(window, cx))
            .bg(tokens.sidebar)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w_full()
                    .h(px(HAIRLINE_PX))
                    .bg(tokens.line),
            )
            .child(self.tabs)
            .test_support()
    }
}

/// The pane's toolbar: search and filters on the leading side, actions on
/// the trailing side. Leading items wrap onto another line in a narrow pane;
/// trailing items stay on the first. The dock switches are the status bar's
/// (see [`super::SpaceDocks`]).
#[derive(IntoElement)]
pub struct Toolbar {
    id: ElementId,
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
    strip: bool,
}

impl Toolbar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            leading: Vec::new(),
            trailing: Vec::new(),
            strip: false,
        }
    }

    /// A leading item, in call order: a breadcrumb, the search field, a
    /// filter.
    pub fn leading(mut self, item: impl IntoElement) -> Self {
        self.leading.push(item.into_any_element());
        self
    }

    /// A trailing item, in call order: the actions for what is open.
    pub fn trailing(mut self, item: impl IntoElement) -> Self {
        self.trailing.push(item.into_any_element());
        self
    }

    /// The toolbar heads a pane that has no tab strip, beside a dock. It is
    /// then the pane's strip: exactly [`strip_height`] high, as the dock
    /// headers are, so the hairlines under them are one line. Its items
    /// keep to that one line and centre on it. Controls that may not fit, in
    /// a narrow pane or at a large icon-button size, go in an
    /// `overflow_toolbar::OverflowToolbar` on either side, which moves the
    /// last of them into its menu. Each side keeps room for that menu's
    /// button, and the gaps between its items, while it holds anything;
    /// beyond that the trailing side yields the rest of the line, so its own
    /// overflow toolbar collapses rather than pushing the leading side out.
    /// The strip clips anything else that runs past it, such as the trailing
    /// side in a strip too narrow for both menu buttons. Trailing content
    /// must shrink, as an overflow toolbar does; content that cannot is
    /// clipped from its start, as the trailing side's items end at its
    /// trailing edge.
    pub fn strip(mut self) -> Self {
        self.strip = true;
        self
    }
}

/// How a toolbar lays out its line: as the pane's strip, exactly `height`
/// high, each side never narrower than `reserve` (room for an overflow
/// menu's button) and the `gap`s between its items while it holds anything;
/// or at least this high with its leading items wrapping.
enum Line {
    Strip {
        height: Pixels,
        reserve: Pixels,
        gap: Pixels,
    },
    Wrapping(Pixels),
}

impl Line {
    fn frame(&self, frame: Stateful<Div>) -> Stateful<Div> {
        match *self {
            // A strip too narrow for both sides' menu buttons clips the
            // trailing one rather than letting it run past the pane.
            Self::Strip { height, .. } => frame.items_center().h(height).overflow_hidden(),
            Self::Wrapping(least) => frame.items_start().min_h(least).py_1(),
        }
    }

    /// The leading side, holding `items` items.
    fn leading(&self, leading: Stateful<Div>, items: usize) -> Stateful<Div> {
        match self {
            Self::Strip { .. } => self.reserved(ring_clip(leading), items),
            Self::Wrapping(_) => leading.flex_wrap(),
        }
    }

    /// The trailing side, holding `items` items, which end at its trailing
    /// edge however much room it keeps.
    fn trailing(&self, trailing: Stateful<Div>, items: usize) -> Stateful<Div> {
        match self {
            Self::Strip { .. } => self
                .reserved(ring_clip(trailing).min_w_0(), items)
                .justify_end(),
            Self::Wrapping(_) => trailing.flex_shrink_0(),
        }
    }

    /// A strip side's least width while it holds anything: its menu
    /// button's room, the gaps between its items, which take that room even
    /// when the item beside the menu's toolbar collapses, and a focus ring
    /// each side.
    fn reserved(&self, side: Stateful<Div>, items: usize) -> Stateful<Div> {
        match *self {
            Self::Strip { reserve, gap, .. } if items > 0 => {
                let gaps = gap * items.saturating_sub(1) as f32;
                side.min_w(reserve + gaps + px(FOCUS_RING_PX + FOCUS_RING_PX))
            }
            _ => side,
        }
    }
}

/// A strip side's clip. It spans the strip's height, which holds a focus
/// ring above and below a control (`strip_height`), and reaches a ring's
/// width past the side's edges, so the rings of its first and last controls
/// show.
fn ring_clip(side: Stateful<Div>) -> Stateful<Div> {
    side.h_full()
        .items_center()
        .mx(-px(FOCUS_RING_PX))
        .px(px(FOCUS_RING_PX))
        .overflow_hidden()
}

impl RenderOnce for Toolbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let metrics = Metrics::current(cx);
        let gap = metrics.gap();
        let line = if self.strip {
            Line::Strip {
                height: strip_height(window, cx),
                reserve: metrics.overflow_width().to_pixels(window.rem_size()),
                gap: gap.to_pixels(window.rem_size()),
            }
        } else {
            Line::Wrapping(tokens.toolbar_height)
        };
        let frame = h_flex()
            .id(self.id)
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .gap(gap)
            .px(text_edge(&tokens))
            .bg(tokens.bg)
            .border_b_1()
            .border_color(tokens.line);
        let (leading_items, trailing_items) = (self.leading.len(), self.trailing.len());
        let leading = h_flex()
            .id("workspace-toolbar-leading")
            .flex_1()
            .min_w_0()
            .gap(gap)
            .children(self.leading);
        let trailing = h_flex()
            .id("workspace-toolbar-trailing")
            .gap(gap)
            .children(self.trailing);
        line.frame(frame)
            .child(line.leading(leading, leading_items).test_support())
            .child(line.trailing(trailing, trailing_items).test_support())
            .test_support()
    }
}
