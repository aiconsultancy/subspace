//! The marks a chart draws: bars, lines, areas, scatter points and slices.
//!
//! Positions come from [`Canvas`] and are relative to the element's own
//! bounds, the space the GPUI Kit plot shapes paint in.

use gpui_kit::component::plot::Curve;
use gpui_kit::component::plot::shape::{Arc, Area, Bar, BarAlignment, Line, Pie};
use gpui_kit::{App, Background, Corners, Hsla, Window, fill, px};
use subspace_charts::{ChartKind, Series};

use crate::chart::frame::{self, Canvas};
use crate::chart::layout;

/// A bar's corner radius, except on a stacked segment.
const BAR_RADIUS: f32 = 2.;
/// A line's width, and the diameter of the dot on each of its points.
const STROKE: f32 = 1.5;
const DOT: f32 = 5.;
/// A scatter mark's radius.
const MARK: f32 = 3.5;
/// How solid an area fill is, alone and in a stack.
const FILL: f32 = 0.3;
const STACKED_FILL: f32 = 0.9;
/// A doughnut's hole, as a share of its radius.
const HOLE: f32 = 0.58;
/// The gap between slices, in radians.
const PAD: f32 = 0.004;

/// Paint a chart drawn as slices around a centre.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a zero base, a diameter from a radius, an empty segment. The sizes, opacities and radii that are choices are named constants."
pub(crate) fn round(canvas: &Canvas, window: &mut Window) {
    let values: Vec<f64> = canvas.model.series.iter().map(frame::slice_value).collect();
    let outer = frame::radius(canvas.area);
    let inner = match canvas.model.kind == ChartKind::Doughnut {
        true => outer * HOLE,
        false => 0.,
    };
    let arc = Arc::new().inner_radius(inner).outer_radius(outer);
    let pie = Pie::new()
        .value(|value: &f64| Some(*value as f32))
        .pad_angle(PAD);
    let bounds = canvas.area.bounds(canvas.bounds.origin);
    for slice in pie.arcs(&values) {
        arc.paint(&slice, color_of(canvas, slice.index), &bounds, window);
    }
}

/// The marks the chart's kind draws.
pub(crate) fn marks(canvas: &Canvas, window: &mut Window, cx: &mut App) {
    match canvas.model.kind {
        ChartKind::Bar => bars(canvas, window, cx),
        ChartKind::Line => lines(canvas, window),
        ChartKind::Area => areas(canvas, window),
        ChartKind::Scatter => scatter(canvas, window),
        ChartKind::Pie | ChartKind::Doughnut => round(canvas, window),
    }
}

/// One rectangle of a bar chart: where it sits across the chart, and the two
/// value positions it spans.
struct Column {
    cross: f32,
    base: f32,
    top: f32,
    color: Hsla,
}

/// Stacked segments or grouped bars, both as rectangles of one width.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a zero base, a diameter from a radius, an empty segment. The sizes, opacities and radii that are choices are named constants."
fn bars(canvas: &Canvas, window: &mut Window, cx: &mut App) {
    let (columns, width) = columns(canvas);
    let radius = match canvas.model.stacked {
        true => px(0.),
        false => px(BAR_RADIUS),
    };
    Bar::new()
        .data(columns)
        .alignment(match canvas.is_horizontal() {
            true => BarAlignment::Left,
            false => BarAlignment::Bottom,
        })
        .band_width(width)
        .cross(|column: &Column| Some(column.cross))
        .base(|column: &Column| column.base)
        .value(|column: &Column| Some(column.top))
        .fill(|column: &Column, _, _| Background::from(column.color))
        .corner_radii(Corners::all(radius))
        .paint(&canvas.bounds, window, cx);
}

/// Every rectangle of a bar chart, and the width they share.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a zero base, a diameter from a radius, an empty segment. The sizes, opacities and radii that are choices are named constants."
fn columns(canvas: &Canvas) -> (Vec<Column>, f32) {
    let count = canvas.model.series.len();
    let mut columns = Vec::new();
    let mut width = 1.;
    for category in 0..canvas.count() {
        let stack = layout::stack(&canvas.model.series, category);
        for series in 0..count {
            let (cross, slot) = place_bar(canvas, category, series);
            width = slot;
            let span = match canvas.model.stacked {
                true => stack.get(series).copied().unwrap_or((0., 0.)),
                false => (0., value_at(canvas, series, category)),
            };
            if span.0 != span.1 {
                columns.push(Column {
                    cross,
                    base: canvas.value_at(span.0),
                    top: canvas.value_at(span.1),
                    color: color_of(canvas, series),
                });
            }
        }
    }
    (columns, width)
}

/// Where one bar sits across the chart, and how wide it is: a stacked segment
/// fills its category's band, a grouped bar takes a slot in it.
fn place_bar(canvas: &Canvas, category: usize, series: usize) -> (f32, f32) {
    let (start, band) = canvas.band_at(category);
    match canvas.model.stacked {
        true => (start, band),
        false => layout::slot(series, canvas.model.series.len(), start, band),
    }
}

/// One series' value for a category, or zero where it has none.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a zero base, a diameter from a radius, an empty segment. The sizes, opacities and radii that are choices are named constants."
fn value_at(canvas: &Canvas, series: usize, category: usize) -> f64 {
    canvas
        .model
        .series
        .get(series)
        .and_then(|one| layout::value(one, category))
        .unwrap_or(0.)
}

/// One projected position of a line or an area.
struct Mark {
    x: f32,
    y: f32,
}

/// A line per series, with a dot on every point.
fn lines(canvas: &Canvas, window: &mut Window) {
    for (index, series) in canvas.model.series.iter().enumerate() {
        let color = color_of(canvas, index);
        Line::new()
            .data(marks_of(canvas, series))
            .x(|mark: &Mark| Some(mark.x))
            .y(|mark: &Mark| Some(mark.y))
            .stroke(color)
            .stroke_width(px(STROKE))
            .curve(Curve::Linear)
            .dot()
            .dot_size(px(DOT))
            .dot_fill(color)
            .paint(&canvas.bounds, window);
    }
}

/// An area per series. Stacked areas are drawn as running totals, largest
/// first, so each one covers the band below it.
fn areas(canvas: &Canvas, window: &mut Window) {
    let opacity = match canvas.model.stacked {
        true => STACKED_FILL,
        false => FILL,
    };
    for (index, series) in canvas.model.series.iter().enumerate().rev() {
        let color = color_of(canvas, index);
        let marks = match canvas.model.stacked {
            true => running_marks(canvas, index),
            false => marks_of(canvas, series),
        };
        Area::new()
            .data(marks)
            .x(|mark: &Mark| Some(mark.x))
            .y1(|mark: &Mark| Some(mark.y))
            .y0(canvas.zero())
            .fill(color.opacity(opacity))
            .stroke(color)
            .curve(Curve::Linear)
            .paint(&canvas.bounds, window);
    }
}

/// A round mark on every point of a scatter plot.
// qual:allow(iosp) reason: "Paints each point as it computes it, like the other mark painters; splitting the loop from the paint calls would allocate a mark list per frame for no clarity."
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a zero base, a diameter from a radius, an empty segment. The sizes, opacities and radii that are choices are named constants."
fn scatter(canvas: &Canvas, window: &mut Window) {
    for (index, series) in canvas.model.series.iter().enumerate() {
        let color = color_of(canvas, index);
        for point in &series.points {
            let bounds = canvas.rect(
                canvas.x_at(point.x) - MARK,
                canvas.value_at(point.y) - MARK,
                MARK * 2.,
                MARK * 2.,
            );
            window.paint_quad(fill(bounds, color).corner_radii(Corners::all(px(MARK))));
        }
    }
}

/// One series' positions: its own points, or one per category it has a value
/// for.
fn marks_of(canvas: &Canvas, series: &Series) -> Vec<Mark> {
    if !series.points.is_empty() {
        let mut marks: Vec<Mark> = series
            .points
            .iter()
            .map(|point| Mark {
                x: canvas.x_at(point.x),
                y: canvas.value_at(point.y),
            })
            .collect();
        marks.sort_by(|left, right| left.x.total_cmp(&right.x));
        return marks;
    }
    (0..canvas.count())
        .filter_map(|category| {
            let value = layout::value(series, category)?;
            Some(Mark {
                x: canvas.centre_at(category),
                y: canvas.value_at(value),
            })
        })
        .collect()
}

/// The running total up to and including one series, which a stacked area
/// draws as its top line.
fn running_marks(canvas: &Canvas, series: usize) -> Vec<Mark> {
    match canvas.model.is_point_chart() {
        true => running_points(canvas, series),
        false => running_categories(canvas, series),
    }
}

/// The running total at every category.
fn running_categories(canvas: &Canvas, series: usize) -> Vec<Mark> {
    (0..canvas.count())
        .map(|category| {
            let stack = layout::stack(&canvas.model.series, category);
            let top = stack.get(series).map_or(0., |(base, top)| top.max(*base));
            Mark {
                x: canvas.centre_at(category),
                y: canvas.value_at(top),
            }
        })
        .collect()
}

/// The running total at every x the point series share.
fn running_points(canvas: &Canvas, series: usize) -> Vec<Mark> {
    layout::x_positions(&canvas.model.series)
        .into_iter()
        .map(|x| Mark {
            x: canvas.x_at(x),
            y: canvas.value_at(layout::running_at(&canvas.model.series, series + 1, x)),
        })
        .collect()
}

/// The colour a series is drawn in.
pub(crate) fn color_of(canvas: &Canvas, index: usize) -> Hsla {
    canvas
        .colors
        .get(index)
        .copied()
        .unwrap_or(canvas.tokens.accent)
}

#[cfg(test)]
mod tests {
    use crate::Tokens;
    use crate::chart::frame::{Canvas, Gutters};
    use crate::chart::marks::{columns, marks_of, running_marks};
    use gpui_kit::{Bounds, point, px, size};
    use subspace_charts::{ChartModel, Series};

    fn canvas_of<'a>(
        model: &'a ChartModel,
        colors: &'a [gpui_kit::Hsla],
        tokens: &'a Tokens,
    ) -> Canvas<'a> {
        Canvas::new(
            model,
            colors,
            tokens,
            Bounds::new(point(px(0.), px(0.)), size(px(206.), px(106.))),
            Gutters {
                left: 6.,
                bottom: 20.,
            },
        )
    }

    fn two_series(stacked: bool) -> ChartModel {
        ChartModel {
            stacked,
            labels: vec!["Mon".into(), "Tue".into()],
            series: vec![
                Series::new("pmr", vec![Some(30.0), Some(10.0)]),
                Series::new("cval", vec![Some(20.0), None]),
            ],
            ..ChartModel::default()
        }
    }

    #[test]
    fn stacked_bars_fill_their_band_and_sit_on_one_another() {
        let model = two_series(true);
        let tokens = Tokens::one_dark();
        let canvas = canvas_of(&model, &[], &tokens);
        let (columns, width) = columns(&canvas);

        assert_eq!(columns.len(), 3, "the gap draws no segment");
        assert_eq!(width, canvas.band_at(0).1, "a segment fills its band");
        assert_eq!(columns[0].cross, columns[1].cross, "both sit on Monday");
        assert_eq!(
            columns[0].top, columns[1].base,
            "the second segment starts where the first ends"
        );
        assert!(columns[0].base > columns[0].top, "pixels grow downward");
    }

    #[test]
    fn grouped_bars_split_the_band_and_grow_from_zero() {
        let model = two_series(false);
        let tokens = Tokens::one_dark();
        let canvas = canvas_of(&model, &[], &tokens);
        let (columns, width) = columns(&canvas);

        assert_eq!(columns.len(), 3);
        assert!(width < canvas.band_at(0).1, "two bars share the band");
        assert!(columns[0].cross < columns[1].cross, "they sit side by side");
        assert_eq!(columns[0].base, canvas.zero());
    }

    #[test]
    fn a_line_skips_a_gap_and_a_stacked_area_follows_the_running_total() {
        let model = two_series(true);
        let tokens = Tokens::one_dark();
        let canvas = canvas_of(&model, &[], &tokens);

        let gapped = marks_of(&canvas, &canvas.model.series[1]);
        assert_eq!(gapped.len(), 1, "the missing value leaves no mark");

        let first = running_marks(&canvas, 0);
        let second = running_marks(&canvas, 1);
        assert_eq!(first.len(), 2);
        assert!(
            second[0].y < first[0].y,
            "the running total is higher up the chart"
        );
    }
}
