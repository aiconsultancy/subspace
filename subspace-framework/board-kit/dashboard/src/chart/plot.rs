//! The chart as a GPUI element: it measures its tick labels, paints itself,
//! and answers what the cursor is over with a tooltip.

use std::rc::Rc;

use gpui_kit::component::plot::label::measure_text_width;
use gpui_kit::component::plot::tooltip::{CrossLine, Tooltip};
use gpui_kit::component::plot::{Plot, PlotElement, TooltipState};
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Hsla, IntoElement, Pixels, Point, SharedString, Window,
    point, px,
};
use subspace_charts::{ChartModel, Orientation};

use crate::Tokens;
use crate::chart::frame::{self, Canvas, Gutters};
use crate::chart::{layout, marks, paint};

/// The gap a tick label keeps from the plot.
const GAP: f32 = 8.;
/// The most of the width the labels down the side may take.
const LABEL_SHARE: f32 = 0.4;
/// The most rows one tooltip lists before it counts the rest.
const MAX_ROWS: usize = 8;

/// A [`ChartModel`] drawn in the room its element is given.
pub(crate) struct ChartPlot {
    id: ElementId,
    model: Rc<ChartModel>,
    colors: Rc<Vec<Hsla>>,
    tokens: Tokens,
    /// Measured in `prepaint`, from the labels this chart actually draws.
    gutters: Gutters,
}

impl ChartPlot {
    pub(crate) fn new(
        id: ElementId,
        model: Rc<ChartModel>,
        colors: Rc<Vec<Hsla>>,
        tokens: Tokens,
    ) -> Self {
        Self {
            id,
            model,
            colors,
            tokens,
            gutters: Gutters::default(),
        }
    }

    fn canvas(&self, bounds: Bounds<Pixels>) -> Canvas<'_> {
        Canvas::new(
            &self.model,
            &self.colors,
            &self.tokens,
            bounds,
            self.gutters,
        )
    }
}

impl IntoElement for ChartPlot {
    type Element = PlotElement<Self>;

    fn into_element(self) -> Self::Element {
        PlotElement::new(self)
    }
}

impl Plot for ChartPlot {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut App,
    ) -> Vec<AnyElement> {
        self.gutters = measure(&self.model, &self.tokens, bounds, window);
        Vec::new()
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let canvas = self.canvas(bounds);
        match self.model.kind.is_round() {
            true => marks::round(&canvas, window),
            false => paint::cartesian(&canvas, window, cx),
        }
    }

    fn tooltip_state(
        &self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        _cx: &App,
    ) -> Option<TooltipState> {
        let canvas = self.canvas(bounds);
        let index = hit(&canvas, position)?;
        Some(TooltipState::new(
            index,
            cross_at(&canvas, index, position),
            Vec::new(),
        ))
    }

    fn tooltip(
        &self,
        state: &TooltipState,
        cursor: Point<Pixels>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnyElement> {
        let canvas = self.canvas(bounds);
        let tooltip = Tooltip::new(cursor, bounds.size).glide(false).gap(px(GAP));
        let tooltip = match (self.model.kind.is_round(), self.model.is_point_chart()) {
            (true, _) => slice_tooltip(&canvas, state.index, tooltip),
            (_, true) => point_tooltip(&canvas, state.index, tooltip),
            _ => category_tooltip(&canvas, state, tooltip),
        }?;
        Some(tooltip.into_any_element())
    }
}

/// What the cursor is over, as an index the tooltip can read back.
fn hit(canvas: &Canvas, position: Point<Pixels>) -> Option<usize> {
    match (canvas.model.kind.is_round(), canvas.model.is_point_chart()) {
        (true, _) => canvas.slice_at(position),
        (_, true) => canvas.point_at(position),
        _ => canvas.category_at(position),
    }
}

/// Where the crosshair marking the hovered thing sits.
fn cross_at(canvas: &Canvas, index: usize, position: Point<Pixels>) -> Point<Pixels> {
    if canvas.model.kind.is_round() {
        return position;
    }
    if canvas.model.is_point_chart() {
        return point_at(canvas, index).unwrap_or(position);
    }
    let centre = px(canvas.centre_at(index));
    match canvas.is_horizontal() {
        true => point(position.x, centre),
        false => point(centre, position.y),
    }
}

/// Where one point of a point chart is drawn.
fn point_at(canvas: &Canvas, index: usize) -> Option<Point<Pixels>> {
    let (series, offset) = frame::locate(&canvas.model.series, index)?;
    let mark = canvas.model.series.get(series)?.points.get(offset)?;
    Some(point(px(canvas.x_at(mark.x)), px(canvas.value_at(mark.y))))
}

/// The hovered category, with every series that has a value for it.
fn category_tooltip(canvas: &Canvas, state: &TooltipState, tooltip: Tooltip) -> Option<Tooltip> {
    let label = canvas.model.labels.get(state.index)?;
    let width = canvas.band_at(state.index).1;
    let mut tooltip =
        tooltip
            .title(label.clone())
            .cross_line(band_line(canvas, state.cross_line, width));
    let rows: Vec<(usize, f64)> = canvas
        .model
        .series
        .iter()
        .enumerate()
        .filter_map(|(index, series)| Some((index, layout::value(series, state.index)?)))
        .collect();
    for (index, value) in rows.iter().take(MAX_ROWS) {
        tooltip = series_row(canvas, *index, *value, tooltip);
    }
    Some(more_row(rows.len(), tooltip))
}

/// The hovered point, with its own x position as the title.
fn point_tooltip(canvas: &Canvas, index: usize, tooltip: Tooltip) -> Option<Tooltip> {
    let (series, offset) = frame::locate(&canvas.model.series, index)?;
    let mark = canvas.model.series.get(series)?.points.get(offset)?;
    let tooltip = tooltip.title(layout::format_x_full(mark.x, canvas.model.x_scale));
    Some(series_row(canvas, series, mark.y, tooltip))
}

/// The hovered slice, with its value and its share of the whole.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a gutter of two gaps, a share out of one hundred. The gaps, shares and row limit that are choices are named constants."
fn slice_tooltip(canvas: &Canvas, index: usize, tooltip: Tooltip) -> Option<Tooltip> {
    let series = canvas.model.series.get(index)?;
    let value = frame::slice_value(series);
    let total: f64 = canvas.model.series.iter().map(frame::slice_value).sum();
    let share = match total > 0. {
        true => format!(" ({:.0}%)", value / total * 100.),
        false => String::new(),
    };
    Some(tooltip.title(series.label.clone()).row(
        marks::color_of(canvas, index),
        series.label.clone(),
        format!(
            "{}{share}",
            layout::format_value(value, &canvas.model.format)
        ),
    ))
}

/// One series' row: its colour, its name and its value.
fn series_row(canvas: &Canvas, index: usize, value: f64, tooltip: Tooltip) -> Tooltip {
    let label = canvas
        .model
        .series
        .get(index)
        .map(|series| series.label.clone())
        .unwrap_or_default();
    tooltip.row(
        marks::color_of(canvas, index),
        label,
        layout::format_value(value, &canvas.model.format),
    )
}

/// A closing row saying how many series the tooltip left out.
fn more_row(rows: usize, tooltip: Tooltip) -> Tooltip {
    match rows > MAX_ROWS {
        true => tooltip.plain_row("and more", format!("{} series", rows - MAX_ROWS)),
        false => tooltip,
    }
}

/// The band that marks the hovered category across the plot.
fn band_line(canvas: &Canvas, at: Point<Pixels>, width: f32) -> CrossLine {
    let area = canvas.area;
    match canvas.is_horizontal() {
        true => CrossLine::new(at)
            .horizontal()
            .h_span(area.left, area.width())
            .band(px(width)),
        false => CrossLine::new(at)
            .span(area.top, area.height())
            .band(px(width)),
    }
}

/// The room the tick labels need, measured from the text this chart draws.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a gutter of two gaps, a share out of one hundred. The gaps, shares and row limit that are choices are named constants."
fn measure(
    model: &ChartModel,
    tokens: &Tokens,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> Gutters {
    if model.kind.is_round() {
        return Gutters::default();
    }
    let font = paint::tick_font(tokens);
    let bottom = f32::from(font) + GAP * 2.;
    let horizontal = model.orientation == Orientation::Horizontal && !model.is_point_chart();
    let widest = match horizontal {
        true => widest_text(model.labels.to_vec(), font, window),
        false => widest_text(tick_texts(model), font, window),
    };
    let cap = f32::from(bounds.size.width) * LABEL_SHARE;
    Gutters {
        left: (widest + GAP).min(cap.max(GAP)),
        bottom,
    }
}

/// The text of every value tick, which the gutter must hold.
fn tick_texts(model: &ChartModel) -> Vec<String> {
    let extent = layout::value_extent(model);
    layout::ticks(extent, layout::TICK_COUNT)
        .iter()
        .map(|tick| layout::format_tick(*tick, &model.format))
        .collect()
}

/// The width of the widest line of `texts`.
fn widest_text(texts: Vec<String>, font: Pixels, window: &mut Window) -> f32 {
    texts
        .into_iter()
        .map(|text| measure_text_width(&SharedString::from(text), font, window))
        .fold(0., f32::max)
}

#[cfg(test)]
mod tests {
    use crate::Tokens;
    use crate::chart::frame::{Canvas, Gutters};
    use crate::chart::plot::{cross_at, hit, tick_texts};
    use gpui_kit::{Bounds, Point, point, px, size};
    use subspace_charts::{ChartKind, ChartModel, Point as DataPoint, Series, ValueFormat};

    fn canvas<'a>(
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

    #[test]
    fn a_hover_over_a_category_marks_its_band_not_the_cursor() {
        let model = ChartModel {
            labels: vec!["Mon".into(), "Tue".into()],
            series: vec![Series::new("pmr", vec![Some(1.0), Some(2.0)])],
            ..ChartModel::default()
        };
        let tokens = Tokens::one_dark();
        let canvas = canvas(&model, &[], &tokens);
        let cursor = Point::new(px(30.), px(40.));

        let index = hit(&canvas, cursor).expect("the cursor is over a category");
        assert_eq!(index, 0);
        let marked = cross_at(&canvas, index, cursor);
        assert_eq!(marked.y, cursor.y, "the band follows the cursor down");
        assert_eq!(marked.x, px(canvas.centre_at(0)));
    }

    #[test]
    fn a_hover_over_a_point_marks_the_point_itself() {
        let model = ChartModel {
            kind: ChartKind::Scatter,
            series: vec![Series {
                points: vec![DataPoint { x: 0., y: 0. }, DataPoint { x: 1., y: 1. }],
                ..Series::new("pmr", Vec::new())
            }],
            ..ChartModel::default()
        };
        let tokens = Tokens::one_dark();
        let canvas = canvas(&model, &[], &tokens);
        let near = Point::new(px(canvas.area.right - 2.), px(canvas.area.top + 2.));

        let index = hit(&canvas, near).expect("the cursor is near a point");
        let marked = cross_at(&canvas, index, near);
        assert_eq!(marked, point(px(canvas.area.right), px(canvas.area.top)));
    }

    #[test]
    fn the_gutter_holds_the_widest_tick_label_the_chart_draws() {
        let model = ChartModel {
            labels: vec!["Mon".into()],
            series: vec![Series::new("pmr", vec![Some(12_500.0)])],
            format: ValueFormat::Currency { code: "USD".into() },
            ..ChartModel::default()
        };

        let texts = tick_texts(&model);
        assert!(texts.contains(&"$0".to_owned()), "{texts:?}");
        assert!(
            texts.iter().any(|text| text.ends_with('k')),
            "big ticks are compact: {texts:?}"
        );
    }
}
