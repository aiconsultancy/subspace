//! The pane: the work area between the docks. A tab strip names what is
//! open, a toolbar holds the search, filters and actions for it, and the
//! content fills the rest.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, Stateful, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::text_edge;
use crate::icon_button::Metrics;
use crate::tokens::Tokens;

/// The tab strip's bottom hairline.
const HAIRLINE_PX: f32 = 1.;

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
/// beside it (`list::render_under`). Either may be left out.
pub fn pane_head(tabs: Option<AnyElement>, toolbar: Option<AnyElement>, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    v_flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .children(tabs.map(|tabs| tab_strip(tabs, &tokens).test_support()))
        .children(toolbar)
}

/// The strip is as tall as a dock's header, and shares the docks' surface so
/// the open tab reads as part of the content below it. Its bottom hairline
/// lies behind the tabs, which fill the strip (`tabs::pane_tab_bar`), so the
/// open tab's fill covers it and joins the content.
fn tab_strip(tabs: AnyElement, tokens: &Tokens) -> Stateful<Div> {
    h_flex()
        .id("workspace-tab-strip")
        .relative()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .min_h(tokens.strip_height)
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
        .child(tabs)
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
}

impl Toolbar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            leading: Vec::new(),
            trailing: Vec::new(),
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
}

impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let gap = Metrics::current(cx).gap();
        h_flex()
            .id(self.id)
            .test_support()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .items_start()
            .min_h(tokens.toolbar_height)
            .gap(gap)
            .px(text_edge(&tokens))
            .py_1()
            .bg(tokens.bg)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                h_flex()
                    .id("workspace-toolbar-leading")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .gap(gap)
                    .children(self.leading),
            )
            .child(
                h_flex()
                    .id("workspace-toolbar-trailing")
                    .test_support()
                    .flex_shrink_0()
                    .gap(gap)
                    .children(self.trailing),
            )
    }
}
