//! The shared native chart: a [`ChartModel`] drawn with GPUI (REQ-44).
//!
//! The model comes from a board that builds it in Rust, or from an agent's
//! Flint chart through `subspace_charts::chartjs`. Series colours come from
//! the theme tokens, in the order of [`palette`], unless the model fixes a
//! series' colour; everything else - the grid, the axes, the labels and the
//! tooltip - uses [`Tokens`] too, so a chart reads in One Dark and in the
//! Shell light theme alike.
//!
//! ```ignore
//! use subspace_dashboard::chart::Chart;
//! use gpui_kit::px;
//!
//! v_flex().child(Chart::new("daily-cost", self.daily.clone()).height(px(260.)))
//! ```
//!
//! [`Chart::fill`] takes the height the parent flex container has left
//! instead, so a chart in a column grows with the window rather than
//! leaving a band of empty frame under it.
//!
//! The id must be unique among its siblings: it keeps the chart's hover and
//! appear state between frames.

mod frame;
mod layout;
mod marks;
mod paint;
mod plot;

use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb,
};
use subspace_charts::ChartModel;

use crate::Tokens;

pub use layout::{format_tick, format_value, format_x_full};

/// The height a chart takes when the caller names none.
pub const DEFAULT_HEIGHT: f32 = 240.;
/// The size of a legend swatch, and the radius of its corners.
const SWATCH: f32 = 8.;
const SWATCH_RADIUS: f32 = 2.;
/// The widest the legend runs before its labels are cut.
const LEGEND_WIDTH: f32 = 160.;
/// The reference title and subtitle sizes, which density scales.
const TITLE_FONT: f32 = 13.;
const SUBTITLE_FONT: f32 = 11.;

/// How far a repeated palette colour is lightened on each turn, so a chart
/// with more series than colours still tells them apart.
const SHADE_STEP: f32 = 0.14;
/// How light or dark a shaded colour may become.
const SHADE_LIMITS: (f32, f32) = (0.25, 0.85);

/// The colours series take, in order, when the model fixes none. Every one is
/// a theme token, so the palette follows the active theme. A theme that gives
/// two tones the same colour contributes it once.
pub fn palette(tokens: &Tokens) -> Vec<Hsla> {
    let mut palette: Vec<Hsla> = Vec::new();
    for color in [
        tokens.accent,
        tokens.green,
        tokens.orange,
        tokens.magenta,
        tokens.blue,
        tokens.red,
        tokens.cyan,
        tokens.yellow,
    ] {
        if !palette.iter().any(|held| same(*held, color)) {
            palette.push(color);
        }
    }
    palette
}

/// How far apart two channels may be and still read as one colour.
const SAME_TOLERANCE: f32 = 0.005;

/// Whether two tones are the same colour, as a theme may map them to one.
fn same(left: Hsla, right: Hsla) -> bool {
    let near = |left: f32, right: f32| (left - right).abs() < SAME_TOLERANCE;
    near(left.h, right.h) && near(left.s, right.s) && near(left.l, right.l)
}

/// The colour series `index` is drawn in: the palette entry for its place,
/// lightened a step on every turn past the end of the palette.
pub fn series_color(index: usize, palette: &[Hsla]) -> Hsla {
    let Some(color) = palette.get(index % palette.len().max(1)) else {
        return Hsla::default();
    };
    let turn = (index / palette.len().max(1)) as f32;
    Hsla {
        l: (color.l + SHADE_STEP * turn).clamp(SHADE_LIMITS.0, SHADE_LIMITS.1),
        ..*color
    }
}

/// The colour of every series of `model`: its own when it has one, else the
/// palette colour for its place.
pub fn colors(model: &ChartModel, tokens: &Tokens) -> Vec<Hsla> {
    let palette = palette(tokens);
    model
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| match series.color {
            Some(color) => Hsla::from(rgb(color)),
            None => series_color(index, &palette),
        })
        .collect()
}

/// How tall a chart is: a fixed height, or the room its parent has left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Height {
    /// Exactly this many pixels.
    Fixed(Pixels),
    /// Everything the parent flex container has left for it. The parent
    /// decides the size, so the plot grows with the window.
    Fill,
}

/// A chart. Give it a name unique among its siblings and a model to draw.
#[derive(IntoElement)]
pub struct Chart {
    name: SharedString,
    model: Rc<ChartModel>,
    height: Height,
    /// An accessible label that replaces the generated description, for a
    /// chart whose surroundings name it better.
    label: Option<SharedString>,
}

// qual:allow(dry, boilerplate) reason: "Three one-line setters on a GPUI
// element. A builder derive would add a dependency and a generated type to
// a widget whose whole surface is these three options."
impl Chart {
    pub fn new(name: impl Into<SharedString>, model: impl Into<Rc<ChartModel>>) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            height: Height::Fixed(px(DEFAULT_HEIGHT)),
            label: None,
        }
    }

    /// The height the chart takes; its width is the room it is given.
    // qual:api - a one-line builder a board calls; the layout it feeds is
    // tested through `Area`.
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Height::Fixed(height);
        self
    }

    /// Take the height the parent has left instead of a fixed one. The
    /// parent must be a flex container that gives the chart a share of it.
    pub fn fill(mut self) -> Self {
        self.height = Height::Fill;
        self
    }

    /// The accessible label, instead of the model's own description.
    // qual:api - a one-line builder; the label it sets is read by the
    // accessibility tree, not by logic.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Applies a chart's height to its frame: a fixed box, or a flex child that
/// takes what is left and may shrink.
fn sized(frame: Div, height: Height) -> Div {
    match height {
        Height::Fixed(height) => frame.h(height),
        Height::Fill => frame.flex_1().min_h_0(),
    }
}

impl RenderOnce for Chart {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let colors = Rc::new(colors(&self.model, &tokens));
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| self.model.description().into());
        let frame = ElementId::Name(format!("{}-chart", self.name).into());
        let height = self.height;
        sized(v_flex().w_full(), height)
            .id(frame)
            .gap_1()
            .role(gpui_kit::accesskit::Role::Image)
            .aria_label(label)
            .children(heading(&self.model, &tokens))
            .child(body(self, colors, &tokens))
    }
}

/// The title and subtitle above the plot, when the model has them.
fn heading(model: &ChartModel, tokens: &Tokens) -> Vec<gpui_kit::AnyElement> {
    let mut lines = Vec::new();
    if !model.title.is_empty() {
        lines.push(
            div()
                .text_size(tokens.heading_size(TITLE_FONT))
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(tokens.fg_strong)
                .child(SharedString::from(model.title.clone()))
                .into_any_element(),
        );
    }
    if !model.subtitle.is_empty() {
        lines.push(
            div()
                .text_size(tokens.heading_size(SUBTITLE_FONT))
                .text_color(tokens.muted)
                .child(SharedString::from(model.subtitle.clone()))
                .into_any_element(),
        );
    }
    lines
}

/// The plot and, beside it, the legend.
fn body(chart: Chart, colors: Rc<Vec<Hsla>>, tokens: &Tokens) -> impl IntoElement {
    let model = chart.model;
    if model.is_empty() {
        return empty(tokens).into_any_element();
    }
    let plot = plot::ChartPlot::new(
        ElementId::Name(chart.name),
        model.clone(),
        colors.clone(),
        *tokens,
    );
    h_flex()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(div().flex_1().min_w_0().h_full().child(plot))
        .children(legend(&model, &colors, tokens))
        .into_any_element()
}

/// What a chart with nothing in it shows.
fn empty(tokens: &Tokens) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(tokens.row_font)
        .text_color(tokens.muted)
        .child("No data")
}

/// The series, named in the colours they are drawn in.
fn legend(model: &ChartModel, colors: &[Hsla], tokens: &Tokens) -> Option<impl IntoElement> {
    if !model.legend || model.series.is_empty() {
        return None;
    }
    let rows = model.series.iter().enumerate().map(|(index, series)| {
        let color = colors.get(index).copied().unwrap_or(tokens.accent);
        h_flex()
            .gap_1p5()
            .items_center()
            .child(div().size(px(SWATCH)).rounded(px(SWATCH_RADIUS)).bg(color))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(series.label.clone())),
            )
    });
    Some(
        v_flex()
            .max_w(px(LEGEND_WIDTH))
            .flex_none()
            .gap_1()
            .justify_center()
            .text_size(tokens.heading_size(SUBTITLE_FONT))
            .text_color(tokens.fg)
            .children(rows),
    )
}

#[cfg(test)]
mod tests {
    use crate::Tokens;
    use crate::chart::{colors, palette};
    use gpui_kit::{Hsla, rgb};
    use subspace_charts::{ChartModel, Series};

    #[test]
    fn the_palette_comes_from_the_theme_tokens_without_a_repeat() {
        let tokens = Tokens::one_dark();
        let one_dark = palette(&tokens);

        assert_eq!(one_dark[0], tokens.accent);
        assert_eq!(
            one_dark.len(),
            8,
            "One Dark maps every tone to its own colour"
        );

        // The Shell dark theme gives `accent` and `blue` one colour.
        let shared = Hsla::from(rgb(0x81ccd9));
        let shell = Tokens {
            accent: shared,
            blue: shared,
            ..tokens
        };
        let shell = palette(&shell);
        assert_eq!(shell.len(), 7, "the shared colour is listed once");
        assert_eq!(shell.iter().filter(|color| **color == shared).count(), 1);
    }

    #[test]
    fn a_series_colour_wins_over_the_palette_and_a_repeat_changes_shade() {
        let tokens = Tokens::one_dark();
        let model = ChartModel {
            series: (0..10)
                .map(|index| Series::new(format!("s{index}"), vec![Some(1.0)]))
                .chain([Series::new("fixed", vec![Some(1.0)]).color(0x36a2eb)])
                .collect(),
            ..ChartModel::default()
        };
        let colors = colors(&model, &tokens);

        assert_eq!(colors[0], tokens.accent);
        assert_eq!(colors[8].h, colors[0].h, "the palette starts again");
        assert!(colors[8].l > colors[0].l, "a second turn is lighter");
        assert_eq!(colors[10], Hsla::from(rgb(0x36a2eb)));
    }
}
