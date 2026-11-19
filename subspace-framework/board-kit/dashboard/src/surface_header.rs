//! The title bar of a Framework view that the shell shows in place of the
//! board, such as Logs and Search. It names the view, carries the view's own
//! actions and always shows Close, so the view never depends on a toolbar's
//! overflow menu or a hidden shortcut to leave.

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_lucide::{IconName, SvgIcon};

use crate::Tokens;
use crate::action::{Intent, icon_button};
use crate::icon_button::{IconButton, Metrics};
use crate::typography::heading_font;

/// The key that closes every surface with this header.
const CLOSE_KEY: &str = "Esc";

type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A view's glyph and title, an optional description, the view's trailing
/// actions and a Close control that never moves into overflow.
#[derive(IntoElement)]
pub struct SurfaceHeader {
    icon: SvgIcon,
    title: SharedString,
    description: Option<SharedString>,
    actions: Vec<AnyElement>,
    close_label: SharedString,
    on_close: CloseHandler,
}

impl SurfaceHeader {
    /// `close_label` names the view, such as "Close logs"; the tooltip adds
    /// the Escape shortcut the owning view binds.
    pub fn new(
        icon: SvgIcon,
        title: impl Into<SharedString>,
        close_label: impl Into<SharedString>,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            description: None,
            actions: Vec::new(),
            close_label: close_label.into(),
            on_close: Rc::new(on_close),
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A view action shown before Close, in call order.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for SurfaceHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let gap = Metrics::current(cx).gap();
        let title = title(&self, &tokens, heading_font(cx));
        let close = close_button(self.close_label, self.on_close);
        h_flex()
            .id("surface-header")
            .test_support()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .min_h(tokens.toolbar_height)
            .gap(gap)
            .px(tokens.pad_x)
            .py_1()
            .bg(tokens.bg)
            .border_b_1()
            .border_color(tokens.line)
            .child(title)
            .child(
                h_flex()
                    .id("surface-actions")
                    .test_support()
                    .flex_shrink_0()
                    .gap(gap)
                    .children(self.actions)
                    .child(close),
            )
    }
}

/// The glyph sits on the title's line; a description starts under the title,
/// past the glyph's slot, so both lines share one text spine.
fn title(header: &SurfaceHeader, tokens: &Tokens, family: SharedString) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .child(
            h_flex()
                .min_w_0()
                .gap_2()
                .child(
                    Icon::from(header.icon)
                        .flex_shrink_0()
                        .size(tokens.heading_font)
                        .text_color(tokens.muted),
                )
                .child(
                    div()
                        .id("surface-title")
                        .test_support()
                        .min_w_0()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(family)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(tokens.heading_font)
                        .text_color(tokens.fg_strong)
                        .child(header.title.clone()),
                ),
        )
        .when_some(header.description.clone(), |column, description| {
            column.child(
                div()
                    .pl(tokens.heading_font)
                    .ml_2()
                    .text_size(tokens.secondary_size())
                    .text_color(tokens.muted)
                    .child(description),
            )
        })
}

fn close_button(label: SharedString, on_close: CloseHandler) -> IconButton {
    let tooltip = close_tooltip(&label);
    icon_button("surface-close", Intent::Quiet, IconName::X, label)
        .tooltip(tooltip)
        .on_click(move |event, window, cx| on_close(event, window, cx))
}

/// "Close logs (Esc)": the control's name and the key that does the same.
fn close_tooltip(label: &str) -> String {
    format!("{label} ({CLOSE_KEY})")
}
