//! Single-line text that is cut with an ellipsis when it does not fit, and
//! shows the whole text in a tooltip when it is.
//!
//! The widget measures: it shapes the text with the window's text system and
//! compares that width with the width it was laid out in, so text that fits
//! carries no tooltip at all. Text the caller already cut in code passes the
//! original through [`Shortened::full`], and always offers it.
//!
//! ```ignore
//! use crate::truncated;
//! truncated::text(("row-title", index), title).text_color(tokens.fg_strong)
//! ```

use std::time::Duration;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Refineable as _, RenderOnce, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, canvas, div, px,
};

/// Longer than GPUI's own 500 ms, so running the pointer down a list does not
/// flash a tooltip over every row.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(600);
/// The widest a tooltip runs before its text wraps.
const TOOLTIP_WIDTH: f32 = 480.;
/// Shaping and layout round differently, so a hair over the available width
/// is not a cut.
const TOLERANCE: f32 = 1.;
/// A width the layout has not reported yet.
const UNMEASURED: f32 = 0.;

/// Whether text of `text_width` is cut inside `available`. A width that is
/// not known yet (zero) is not a cut, so nothing flashes before the first
/// measurement.
pub fn is_cut(text_width: f32, available: f32) -> bool {
    if available <= UNMEASURED || text_width <= UNMEASURED {
        return false;
    }
    text_width > available + TOLERANCE
}

/// Single-line text with a full-text tooltip once it is cut.
pub fn text(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Shortened {
    Shortened {
        id: id.into(),
        text: text.into(),
        full: None,
        child: None,
        wrap: false,
        style: StyleRefinement::default(),
    }
}

/// The same, for text an element already styles, such as a row title with
/// its search matches highlighted. The plain `text` is what gets measured
/// and what the tooltip shows.
pub fn styled(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    child: impl IntoElement,
) -> Shortened {
    Shortened {
        child: Some(child.into_any_element()),
        ..text(id, label)
    }
}

#[derive(IntoElement)]
pub struct Shortened {
    id: ElementId,
    text: SharedString,
    full: Option<SharedString>,
    child: Option<AnyElement>,
    wrap: bool,
    style: StyleRefinement,
}

impl Shortened {
    /// Wrapped text instead of one cut line, for the rows a dashboard shows
    /// in its wrapped view. Wrapped text is never cut, so it offers no
    /// tooltip.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// The text before the caller cut it, which the tooltip shows. Text cut
    /// in code always offers its original, however wide the row is.
    pub fn full(mut self, full: impl Into<SharedString>) -> Self {
        self.full = Some(full.into());
        self
    }
}

impl Styled for Shortened {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Shortened {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let cut = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let full = self.full.unwrap_or_else(|| self.text.clone());
        let offers = !self.wrap && (*cut.read(cx) || full != self.text);
        let wrap = self.wrap;
        let width = shaped_width(window, &self.text);
        let shown_text = self
            .child
            .unwrap_or_else(|| self.text.clone().into_any_element());
        let mut shown = div()
            .id(self.id)
            .relative()
            .min_w_0()
            .map(|shown| {
                if wrap {
                    shown.whitespace_normal()
                } else {
                    shown.overflow_hidden().whitespace_nowrap().text_ellipsis()
                }
            })
            .child(shown_text)
            .when(!wrap, |shown| shown.child(measure(width, cut)))
            .when(offers, |shown| {
                shown
                    .tooltip(move |window, cx| wrapped(full.clone(), window, cx))
                    .tooltip_show_delay(TOOLTIP_DELAY)
            });
        shown.style().refine(&self.style);
        shown
    }
}

/// Records whether the text was cut in the width it was given. It sits over
/// the text, so it changes no layout.
fn measure(width: Pixels, cut: Entity<bool>) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            let measured = is_cut(f32::from(width), f32::from(bounds.size.width));
            cut.update(cx, |cut, cx| {
                if *cut != measured {
                    *cut = measured;
                    cx.notify();
                }
            });
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

/// The width this text would take if nothing cut it.
fn shaped_width(window: &Window, text: &SharedString) -> Pixels {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    // The shaper takes one line, and these are one-line labels by definition.
    let line: SharedString = first_line(text).to_owned().into();
    let run = style.to_run(line.len());
    window
        .text_system()
        .shape_line(line, font_size, &[run], None)
        .width
}

/// The first line of a label, which is all a single-line row can show.
pub fn first_line(text: &str) -> &str {
    text.split('\n').next().unwrap_or_default()
}

/// A tooltip that wraps, so a very long title stays readable.
fn wrapped(full: SharedString, window: &mut Window, cx: &mut App) -> AnyView {
    Tooltip::element(move |_, _| {
        div()
            .max_w(px(TOOLTIP_WIDTH))
            .whitespace_normal()
            .child(full.clone())
    })
    .build(window, cx)
}

#[cfg(test)]
mod tests {
    use super::{first_line, is_cut};

    #[test]
    fn only_text_wider_than_its_row_is_cut() {
        assert!(is_cut(200., 120.));
        assert!(!is_cut(100., 120.));
        assert!(!is_cut(120., 120.), "text that just fits is not cut");
    }

    #[test]
    fn rounding_between_shaping_and_layout_is_not_a_cut() {
        assert!(!is_cut(120.5, 120.));
        assert!(is_cut(122., 120.));
    }

    #[test]
    fn an_unmeasured_row_offers_no_tooltip() {
        assert!(!is_cut(200., 0.), "before the first layout");
        assert!(!is_cut(0., 120.), "empty text");
        assert!(!is_cut(200., -5.), "a row with no room at all");
    }

    #[test]
    fn a_row_measures_the_line_it_can_show() {
        assert_eq!(first_line("one line"), "one line");
        assert_eq!(first_line("first\nsecond"), "first");
        assert_eq!(first_line(""), "");
    }
}
