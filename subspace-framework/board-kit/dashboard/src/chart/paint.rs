//! Painting a chart's frame: the grid, the axes and their tick labels. The
//! marks themselves are in [`crate::chart::marks`].
//!
//! The shapes come from GPUI Kit's plot primitives, which take positions in
//! pixels from the bounds they are painted in; every position here is
//! relative to the element's own bounds, which [`Canvas`] holds.

use gpui_kit::component::plot::label::{PlotLabel, Text};
use gpui_kit::{App, Pixels, TextAlign, Window, fill, point, px};
use subspace_charts::AxisScale;

use crate::Tokens;
use crate::chart::frame::Canvas;
use crate::chart::{layout, marks};

/// A grid line's thickness.
const LINE: f32 = 1.;
/// How much of the border colour a grid line keeps.
const GRID_OPACITY: f32 = 0.5;
/// The gap between a tick label and what it labels.
const LABEL_GAP: f32 = 6.;
/// The reference tick-label size, which density scales.
pub(crate) const TICK_FONT: f32 = 10.;
/// The room one category label needs across the bottom.
const LABEL_ROOM: f32 = 58.;
/// The room one category label needs down the side.
const ROW_ROOM: f32 = 18.;
/// The x ticks a point chart draws.
const X_TICKS: usize = 5;

/// The tick-label size the active density asks for.
pub(crate) fn tick_font(tokens: &Tokens) -> Pixels {
    tokens.heading_size(TICK_FONT)
}

/// Paint a chart drawn on axes.
pub(crate) fn cartesian(canvas: &Canvas, window: &mut Window, cx: &mut App) {
    grid(canvas, window);
    value_labels(canvas, window, cx);
    axis_labels(canvas, window, cx);
    marks::marks(canvas, window, cx);
}

/// One line across the plot for every value tick, and a stronger one on zero.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a label centred on half its height, a zero line. The gaps, sizes and tick counts that are choices are named constants."
fn grid(canvas: &Canvas, window: &mut Window) {
    let faint = canvas.tokens.line.opacity(GRID_OPACITY);
    for tick in &canvas.ticks {
        window.paint_quad(fill(value_line(canvas, *tick), faint));
    }
    window.paint_quad(fill(value_line(canvas, 0.), canvas.tokens.line));
}

/// The line across the plot at one value.
fn value_line(canvas: &Canvas, value: f64) -> gpui_kit::Bounds<Pixels> {
    let at = canvas.value_at(value);
    let area = canvas.area;
    match canvas.is_horizontal() {
        true => canvas.rect(at, area.top, LINE, area.height()),
        false => canvas.rect(area.left, at, area.width(), LINE),
    }
}

/// The value ticks, in the gutter beside the plot.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a label centred on half its height, a zero line. The gaps, sizes and tick counts that are choices are named constants."
fn value_labels(canvas: &Canvas, window: &mut Window, cx: &mut App) {
    let font = tick_font(canvas.tokens);
    let color = canvas.tokens.muted;
    let labels = canvas
        .ticks
        .iter()
        .map(|tick| {
            let text = layout::format_tick(*tick, &canvas.model.format);
            let at = canvas.value_at(*tick);
            match canvas.is_horizontal() {
                true => Text::new(
                    text,
                    point(px(at), px(canvas.area.bottom + LABEL_GAP)),
                    color,
                )
                .align(TextAlign::Center),
                false => Text::new(
                    text,
                    point(
                        px(canvas.area.left - LABEL_GAP),
                        px(at - f32::from(font) / 2.),
                    ),
                    color,
                )
                .align(TextAlign::Right),
            }
            .font_size(font)
        })
        .collect();
    PlotLabel::new(labels).paint(&canvas.bounds, window, cx);
}

/// The labels on the other axis: the categories, or a point chart's x ticks.
fn axis_labels(canvas: &Canvas, window: &mut Window, cx: &mut App) {
    let labels = match canvas.model.is_point_chart() {
        true => x_labels(canvas),
        false => category_labels(canvas),
    };
    PlotLabel::new(labels).paint(&canvas.bounds, window, cx);
}

/// The category labels, thinned until they fit.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a label centred on half its height, a zero line. The gaps, sizes and tick counts that are choices are named constants."
fn category_labels(canvas: &Canvas) -> Vec<Text> {
    let font = tick_font(canvas.tokens);
    let color = canvas.tokens.muted;
    let horizontal = canvas.is_horizontal();
    let room = match horizontal {
        true => ROW_ROOM,
        false => LABEL_ROOM,
    };
    let stride = layout::label_stride(canvas.count(), canvas.category_length(), room);
    canvas
        .model
        .labels
        .iter()
        .enumerate()
        .filter(|(index, _)| index % stride == 0)
        .map(|(index, label)| {
            let at = canvas.centre_at(index);
            match horizontal {
                true => Text::new(
                    label.clone(),
                    point(
                        px(canvas.area.left - LABEL_GAP),
                        px(at - f32::from(font) / 2.),
                    ),
                    color,
                )
                .align(TextAlign::Right),
                false => Text::new(
                    label.clone(),
                    point(px(at), px(canvas.area.bottom + LABEL_GAP)),
                    color,
                )
                .align(TextAlign::Center),
            }
            .font_size(font)
        })
        .collect()
}

/// The x ticks of a point chart: round numbers, or dates spread evenly
/// across the extent, which round numbers cannot express.
fn x_labels(canvas: &Canvas) -> Vec<Text> {
    let font = tick_font(canvas.tokens);
    let (low, high) = canvas.x_extent;
    let positions = match canvas.model.x_scale {
        AxisScale::Number => layout::ticks(canvas.x_extent, X_TICKS),
        AxisScale::Timestamp => spread(canvas.x_extent, X_TICKS),
    };
    positions
        .into_iter()
        .filter(|value| (low..=high).contains(value))
        .map(|value| {
            Text::new(
                layout::format_x(value, canvas.model.x_scale),
                point(px(canvas.x_at(value)), px(canvas.area.bottom + LABEL_GAP)),
                canvas.tokens.muted,
            )
            .align(TextAlign::Center)
            .font_size(font)
        })
        .collect()
}

/// `count` positions evenly spaced across an extent, both ends included.
fn spread((low, high): (f64, f64), count: usize) -> Vec<f64> {
    let steps = count.max(2) - 1;
    (0..=steps)
        .map(|step| low + (high - low) * step as f64 / steps as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::Tokens;
    use crate::chart::paint::{spread, tick_font};
    use gpui_kit::px;

    #[test]
    fn tick_labels_follow_the_density_and_dates_spread_evenly() {
        let tokens = Tokens::one_dark();
        assert_eq!(tick_font(&tokens), px(10.));
        assert_eq!(spread((0., 100.), 5), [0., 25., 50., 75., 100.]);
        assert_eq!(spread((10., 10.), 1), [10., 10.]);
    }
}
