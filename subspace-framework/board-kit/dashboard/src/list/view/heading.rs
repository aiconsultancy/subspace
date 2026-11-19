//! The title and context at the top of a list Space.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, TextStyle, Window, div,
};

use crate::tokens::Tokens;
use crate::typography::heading_font;

/// Titles read one step above group headings.
const TITLE_SCALE: f32 = 1.25;

/// Space title and context on one line, aligned with the content below.
pub fn header(
    id: &str,
    title: impl Into<SharedString>,
    scope: impl Into<SharedString>,
    inset: Pixels,
    _: &App,
) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .min_w_0()
        .px(inset)
        .pt_4()
        .pb_2()
        .child(HeadingLine {
            id: id.into(),
            title: title.into(),
            scope: scope.into(),
        })
}

#[derive(IntoElement)]
struct HeadingLine {
    id: SharedString,
    title: SharedString,
    scope: SharedString,
}

impl RenderOnce for HeadingLine {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let title_size = tokens.heading_font * TITLE_SCALE;
        let scope_size = tokens.secondary_size();
        let family = heading_font(cx);
        let mut scope_style = window.text_style();
        scope_style.font_size = scope_size.into();
        let mut title_style = scope_style.clone();
        title_style.font_size = title_size.into();
        title_style.font_family = family.clone();
        title_style.font_weight = FontWeight::BOLD;
        let title_baseline = baseline(&self.title, &title_style, window);
        let scope_baseline = baseline(&self.scope, &scope_style, window);
        let aligned_baseline = title_baseline.max(scope_baseline);

        h_flex()
            .items_start()
            .gap_2()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .id(SharedString::from(format!("{}-heading", self.id)))
                    .role(Role::Heading)
                    .aria_label(self.title.clone())
                    .test_support()
                    .flex_shrink_0()
                    .max_w_full()
                    .mt(aligned_baseline - title_baseline)
                    .truncate()
                    .font_family(family)
                    .text_size(title_size)
                    .line_height(title_style.line_height_in_pixels(window.rem_size()))
                    .font_weight(FontWeight::BOLD)
                    .text_color(tokens.fg_strong)
                    .child(self.title),
            )
            .children((!self.scope.is_empty()).then(|| {
                div()
                    .id(SharedString::from(format!("{}-scope", self.id)))
                    .aria_label(self.scope.clone())
                    .test_support()
                    .min_w_0()
                    .mt(aligned_baseline - scope_baseline)
                    .truncate()
                    .text_size(scope_size)
                    .line_height(scope_style.line_height_in_pixels(window.rem_size()))
                    .text_color(tokens.muted)
                    .child(self.scope)
            }))
    }
}

/// GPUI's flex baseline follows the label boxes. Use the shaped line's
/// ascent and descent to align the glyphs inside those boxes.
fn baseline(text: &str, style: &TextStyle, window: &Window) -> Pixels {
    const HALF: f32 = 0.5;
    let text = text.lines().next().unwrap_or_default();
    let rem_size = window.rem_size();
    let line = window.text_system().layout_line(
        text,
        style.font_size.to_pixels(rem_size),
        &[style.to_run(text.len())],
        None,
    );
    (style.line_height_in_pixels(rem_size) + line.ascent - line.descent) * HALF
}
