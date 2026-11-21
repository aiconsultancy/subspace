//! Scrolling detail content and the optional fixed footer below it.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

const TOP_PADDING_PX: f32 = 16.;
/// Keeps the last line clear of the panel edge.
const BOTTOM_PADDING_PX: f32 = 48.;
const FOOTER_GAP_PX: f32 = 8.;

/// Scrolling panel content with an optional fixed footer, such as a chat composer.
pub struct PanelBody {
    pub content: AnyElement,
    pub footer: Option<AnyElement>,
    pub scroll: Option<ScrollHandle>,
    /// The content supplies its own bounded viewport; the frame must not scroll.
    pub content_owns_scroll: bool,
}

impl<E: IntoElement> From<E> for PanelBody {
    fn from(content: E) -> Self {
        Self {
            content: content.into_any_element(),
            footer: None,
            scroll: None,
            content_owns_scroll: false,
        }
    }
}

#[derive(IntoElement)]
pub(super) struct Frame(PanelBody);
pub(super) fn render(body: PanelBody) -> Frame {
    Frame(body)
}
impl RenderOnce for Frame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        content(self.0, window, cx)
    }
}
fn content(body: PanelBody, window: &mut Window, cx: &mut App) -> Div {
    let bottom_padding = if body.footer.is_some() {
        FOOTER_GAP_PX
    } else {
        BOTTOM_PADDING_PX
    };
    let owner = body.scroll.clone().unwrap_or_else(|| {
        window
            .use_keyed_state("panel-body-scroll", cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone()
    });
    let scroll = div()
        .id("detail-body")
        .test_support()
        .flex_1()
        .min_h_0()
        .map(|pane| {
            if body.content_owns_scroll {
                pane.flex().flex_col().overflow_hidden()
            } else {
                pane.overflow_y_scroll().track_scroll(&owner)
            }
        })
        .pt(px(TOP_PADDING_PX))
        // The body starts on the workspace's text edge, as the title does.
        .px(crate::workspace::text_edge(&crate::Tokens::current(cx)))
        .pb(px(bottom_padding))
        .child(if body.content_owns_scroll {
            body.content
        } else {
            crate::find_in_page::scroll(owner.clone(), body.content).into_any_element()
        });
    v_flex()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .child(scroll)
        .children(
            body.footer
                .map(|footer| div().flex_shrink_0().child(footer)),
        )
}

#[cfg(test)]
#[path = "body_tests.rs"]
mod tests;
