//! Where a chart's parts sit: the plot area inside its tick-label gutters,
//! the pixel each value and category maps to, and what the cursor is over.
//!
//! The arithmetic is [`super::layout`]'s; this module only holds the frame it
//! works in, so the painting code reads one value at a time.

use gpui_kit::{Bounds, Hsla, Pixels, Point, Size, px, size};
use subspace_charts::{ChartModel, Orientation, Series};

use crate::Tokens;
use crate::chart::layout;

/// The breathing room the plot keeps at the edges it has no labels on.
const EDGE: f32 = 6.;
/// How near the cursor must be to a point before its tooltip shows.
const POINT_REACH: f32 = 24.;

/// A length in pixels as a plain number.
fn length(pixels: Pixels) -> f32 {
    f32::from(pixels)
}

/// Room the chart keeps outside the plot area for its tick labels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Gutters {
    pub left: f32,
    pub bottom: f32,
}

/// The plot area, in pixels from the element's top left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    /// The area an element of `size` leaves inside `gutters`.
    // qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
    pub fn new(size: Size<Pixels>, gutters: Gutters) -> Self {
        let width = length(size.width);
        let height = length(size.height);
        let left = gutters.left;
        let top = EDGE;
        Self {
            left,
            top,
            right: (width - EDGE).max(left + 1.),
            bottom: (height - gutters.bottom).max(top + 1.),
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// The area as window bounds, for a shape that centres itself in them.
    // qual:allow(iosp) reason: "A two-line adapter from the area's own numbers to the window bounds a plot shape paints in; splitting the subtraction from the construction would hide what it builds."
    pub fn bounds(&self, origin: Point<Pixels>) -> Bounds<Pixels> {
        let corner = Point::new(px(self.left), px(self.top));
        let extent = size(px(self.right - self.left), px(self.bottom - self.top));
        Bounds::new(origin + corner, extent)
    }
}

/// One chart, ready to paint: its model, its colours and its geometry.
pub(crate) struct Canvas<'a> {
    pub model: &'a ChartModel,
    pub colors: &'a [Hsla],
    pub tokens: &'a Tokens,
    /// The element's own bounds, which every painted position is relative to.
    pub bounds: Bounds<Pixels>,
    pub area: Area,
    /// The extent of the value axis.
    pub extent: (f64, f64),
    /// The extent of a point chart's x axis.
    pub x_extent: (f64, f64),
    pub ticks: Vec<f64>,
}

impl<'a> Canvas<'a> {
    pub fn new(
        model: &'a ChartModel,
        colors: &'a [Hsla],
        tokens: &'a Tokens,
        bounds: Bounds<Pixels>,
        gutters: Gutters,
    ) -> Self {
        let extent = layout::value_extent(model);
        Self {
            model,
            colors,
            tokens,
            bounds,
            area: Area::new(bounds.size, gutters),
            extent,
            x_extent: x_extent(&model.series),
            ticks: layout::ticks(extent, layout::TICK_COUNT),
        }
    }

    /// Whether the categories run down the side. Point charts always have
    /// their values up the side, whatever the configuration asked for.
    pub fn is_horizontal(&self) -> bool {
        self.model.orientation == Orientation::Horizontal && !self.model.is_point_chart()
    }

    /// Where `value` sits along the value axis.
    pub fn value_at(&self, value: f64) -> f32 {
        match self.is_horizontal() {
            true => layout::place(value, self.extent, self.area.left, self.area.right),
            false => layout::place(value, self.extent, self.area.bottom, self.area.top),
        }
    }

    /// Where a point chart's `x` sits across the plot.
    pub fn x_at(&self, x: f64) -> f32 {
        layout::place(x, self.x_extent, self.area.left, self.area.right)
    }

    /// Where the zero line sits along the value axis.
    pub fn zero(&self) -> f32 {
        self.value_at(0.)
    }

    /// A rectangle inside the element, for a line or a mark painted by hand.
    /// Its width and height are already the right way round.
    // qual:allow(iosp) reason: "A two-line adapter from element-relative pixels to window bounds; splitting the offset from the construction would hide what it builds."
    pub fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        let corner = Point::new(px(x), px(y));
        let extent = size(px(width), px(height));
        Bounds::new(self.bounds.origin + corner, extent)
    }

    /// The length of the category axis.
    pub fn category_length(&self) -> f32 {
        match self.is_horizontal() {
            true => self.area.height(),
            false => self.area.width(),
        }
    }

    /// Where the category axis starts.
    pub fn category_start(&self) -> f32 {
        match self.is_horizontal() {
            true => self.area.top,
            false => self.area.left,
        }
    }

    /// The band a category takes along the category axis, as its start and
    /// its width.
    pub fn band_at(&self, index: usize) -> (f32, f32) {
        let (start, width) = layout::band(index, self.count(), self.category_length());
        (self.category_start() + start, width)
    }

    /// The middle of a category's band, which a line point sits on.
    pub fn centre_at(&self, index: usize) -> f32 {
        self.category_start() + layout::centre(index, self.count(), self.category_length())
    }

    /// How many categories the chart has.
    pub fn count(&self) -> usize {
        self.model.labels.len()
    }

    /// The category the cursor is nearest, when it is over the plot.
    // qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
    pub fn category_at(&self, position: Point<Pixels>) -> Option<usize> {
        let along = match self.is_horizontal() {
            true => length(position.y),
            false => length(position.x),
        };
        let length = self.category_length();
        let count = self.count();
        if count == 0 || length <= 0. {
            return None;
        }
        let offset = along - self.category_start();
        let index = (offset / (length / count as f32)).floor();
        (index >= 0.).then(|| (index as usize).min(count - 1))
    }

    /// The point of a point chart nearest the cursor, as a flat index over
    /// every series, when the cursor is near enough to one.
    pub fn point_at(&self, position: Point<Pixels>) -> Option<usize> {
        let (x, y) = (length(position.x), length(position.y));
        let mut nearest = (f32::INFINITY, 0);
        for (index, point) in flat_points(&self.model.series) {
            let distance = (self.x_at(point.x) - x).hypot(self.value_at(point.y) - y);
            if distance < nearest.0 {
                nearest = (distance, index);
            }
        }
        (nearest.0 <= POINT_REACH).then_some(nearest.1)
    }

    /// The slice of a round chart under the cursor.
    // qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
    pub fn slice_at(&self, position: Point<Pixels>) -> Option<usize> {
        let totals: Vec<f64> = self.model.series.iter().map(slice_value).collect();
        let sum: f64 = totals.iter().sum();
        if sum <= 0. {
            return None;
        }
        let share = clock_share(position, self.area)?;
        let mut walked = 0_f32;
        for (index, value) in totals.iter().enumerate() {
            walked += (value / sum) as f32;
            if share <= walked {
                return Some(index);
            }
        }
        Some(totals.len().saturating_sub(1))
    }
}

/// One slice's value: a round chart holds one value per series.
pub(crate) fn slice_value(series: &Series) -> f64 {
    series
        .values
        .iter()
        .filter_map(|value| *value)
        .filter(|value| *value > 0.)
        .sum()
}

/// How far clockwise from twelve o'clock the cursor is, as a share of a turn,
/// when it is inside the chart's circle.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
fn clock_share(position: Point<Pixels>, area: Area) -> Option<f32> {
    let radius = radius(area);
    let centre_x = area.left + area.width() / 2.;
    let centre_y = area.top + area.height() / 2.;
    let x = length(position.x) - centre_x;
    let y = length(position.y) - centre_y;
    if x.hypot(y) > radius {
        return None;
    }
    let turns = x.atan2(-y) / std::f32::consts::TAU;
    Some(match turns < 0. {
        true => turns + 1.,
        false => turns,
    })
}

/// The radius a round chart draws at inside `area`.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
pub(crate) fn radius(area: Area) -> f32 {
    (area.width().min(area.height()) / 2. - EDGE).max(1.)
}

/// Every point of every series, with its flat index.
pub(crate) fn flat_points(series: &[Series]) -> Vec<(usize, subspace_charts::Point)> {
    series
        .iter()
        .flat_map(|one| one.points.iter().copied())
        .enumerate()
        .collect()
}

/// The series and the point a flat index names.
pub(crate) fn locate(series: &[Series], flat: usize) -> Option<(usize, usize)> {
    let mut remaining = flat;
    for (index, one) in series.iter().enumerate() {
        if remaining < one.points.len() {
            return Some((index, remaining));
        }
        remaining -= one.points.len();
    }
    None
}

/// The extent of every point's x position, widened when they share one.
// qual:allow(complexity, magic_numbers) reason: "Chart geometry: the numbers are the arithmetic itself - a centre at half the side, a plot at least one pixel wide, a zero total. The edge and reach that are choices are named constants."
fn x_extent(series: &[Series]) -> (f64, f64) {
    let positions = series.iter().flat_map(|one| one.points.iter());
    let (low, high) = positions.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), point| {
        (low.min(point.x), high.max(point.x))
    });
    match low.is_finite() && high > low {
        true => (low, high),
        false => (low.min(0.), low.max(0.) + 1.),
    }
}

#[cfg(test)]
mod tests {
    use crate::Tokens;
    use crate::chart::frame::{Area, Canvas, Gutters, locate, radius};
    use gpui_kit::{Bounds, Hsla, Point, point, px, size};
    use subspace_charts::{ChartKind, ChartModel, Orientation, Point as DataPoint, Series};

    fn bounds() -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(206.), px(106.)))
    }

    fn plot<'a>(model: &'a ChartModel, colors: &'a [Hsla], tokens: &'a Tokens) -> Canvas<'a> {
        Canvas::new(
            model,
            colors,
            tokens,
            bounds(),
            Gutters {
                left: 6.,
                bottom: 20.,
            },
        )
    }

    fn category_model(stacked: bool) -> ChartModel {
        ChartModel {
            stacked,
            labels: vec!["Mon".into(), "Tue".into()],
            series: vec![Series::new("pmr", vec![Some(50.0), Some(100.0)])],
            ..ChartModel::default()
        }
    }

    #[test]
    fn the_area_sits_inside_the_gutters() {
        let area = Area::new(
            bounds().size,
            Gutters {
                left: 30.,
                bottom: 20.,
            },
        );

        assert_eq!(area.left, 30.);
        assert_eq!(area.top, 6.);
        assert_eq!(area.right, 200.);
        assert_eq!(area.bottom, 86.);
        assert_eq!(area.width(), 170.);
        assert_eq!(area.height(), 80.);
    }

    #[test]
    fn an_area_smaller_than_its_gutters_still_has_a_plot() {
        let area = Area::new(
            size(px(10.), px(10.)),
            Gutters {
                left: 40.,
                bottom: 40.,
            },
        );

        assert!(area.right > area.left);
        assert!(area.bottom > area.top);
        assert!(radius(area) >= 1.);
    }

    #[test]
    fn values_run_up_the_side_and_categories_across_the_bottom() {
        let model = category_model(false);
        let tokens = Tokens::one_dark();
        let canvas = plot(&model, &[], &tokens);

        assert!(!canvas.is_horizontal());
        assert_eq!(canvas.extent, (0., 100.));
        assert_eq!(canvas.zero(), canvas.area.bottom);
        assert_eq!(canvas.value_at(100.), canvas.area.top);
        assert_eq!(canvas.category_length(), canvas.area.width());

        let (start, width) = canvas.band_at(0);
        assert!(start >= canvas.area.left);
        assert!(start + width <= canvas.area.right + 0.01);
        assert!(canvas.centre_at(0) < canvas.centre_at(1));
    }

    #[test]
    fn a_horizontal_chart_swaps_the_axes_but_a_point_chart_does_not() {
        let tokens = Tokens::one_dark();
        let horizontal = ChartModel {
            orientation: Orientation::Horizontal,
            ..category_model(false)
        };
        let canvas = plot(&horizontal, &[], &tokens);
        assert!(canvas.is_horizontal());
        assert_eq!(canvas.zero(), canvas.area.left);
        assert_eq!(canvas.value_at(100.), canvas.area.right);
        assert_eq!(canvas.category_length(), canvas.area.height());

        let points = ChartModel {
            kind: ChartKind::Scatter,
            orientation: Orientation::Horizontal,
            labels: Vec::new(),
            series: vec![Series {
                points: vec![DataPoint { x: 1., y: 2. }, DataPoint { x: 3., y: 8. }],
                ..Series::new("pmr", Vec::new())
            }],
            ..ChartModel::default()
        };
        let canvas = plot(&points, &[], &tokens);
        assert!(!canvas.is_horizontal(), "a point chart keeps its axes");
        assert_eq!(canvas.x_extent, (1., 3.));
        assert_eq!(canvas.x_at(1.), canvas.area.left);
        assert_eq!(canvas.x_at(3.), canvas.area.right);
    }

    #[test]
    fn the_cursor_finds_the_category_the_point_and_the_slice_under_it() {
        let model = category_model(true);
        let tokens = Tokens::one_dark();
        let canvas = plot(&model, &[], &tokens);
        let at = |x: f32, y: f32| Point::new(px(x), px(y));

        assert_eq!(canvas.category_at(at(20., 40.)), Some(0));
        assert_eq!(canvas.category_at(at(190., 40.)), Some(1));
        assert_eq!(canvas.category_at(at(300., 40.)), Some(1), "past the end");
        assert_eq!(canvas.category_at(at(0., 40.)), None, "left of the plot");

        let points = ChartModel {
            kind: ChartKind::Scatter,
            labels: Vec::new(),
            series: vec![Series {
                points: vec![DataPoint { x: 0., y: 0. }, DataPoint { x: 1., y: 1. }],
                ..Series::new("pmr", Vec::new())
            }],
            ..ChartModel::default()
        };
        let canvas = plot(&points, &[], &tokens);
        assert_eq!(
            canvas.point_at(at(canvas.area.left, canvas.area.bottom)),
            Some(0)
        );
        assert_eq!(
            canvas.point_at(at(canvas.area.right, canvas.area.top)),
            Some(1)
        );
        assert_eq!(canvas.point_at(at(100., 50.)), None, "too far from both");
    }

    #[test]
    fn a_round_chart_reads_its_slice_from_the_angle() {
        let model = ChartModel {
            kind: ChartKind::Pie,
            labels: vec!["a".into(), "b".into()],
            series: vec![
                Series::new("a", vec![Some(3.0)]),
                Series::new("b", vec![Some(1.0)]),
            ],
            ..ChartModel::default()
        };
        let tokens = Tokens::one_dark();
        let canvas = Canvas::new(&model, &[], &tokens, bounds(), Gutters::default());
        let centre_x = canvas.area.left + canvas.area.width() / 2.;
        let centre_y = canvas.area.top + canvas.area.height() / 2.;

        let right = Point::new(px(centre_x + 10.), px(centre_y));
        assert_eq!(
            canvas.slice_at(right),
            Some(0),
            "a quarter turn is in the first slice"
        );
        let up_left = Point::new(px(centre_x - 10.), px(centre_y - 10.));
        assert_eq!(
            canvas.slice_at(up_left),
            Some(1),
            "the last eighth of the turn is in the second slice"
        );
        let outside = Point::new(px(0.), px(0.));
        assert_eq!(canvas.slice_at(outside), None);
    }

    #[test]
    fn a_flat_point_index_names_its_series_and_its_point() {
        let series = [
            Series {
                points: vec![DataPoint { x: 0., y: 0. }, DataPoint { x: 1., y: 1. }],
                ..Series::new("pmr", Vec::new())
            },
            Series {
                points: vec![DataPoint { x: 2., y: 2. }],
                ..Series::new("cval", Vec::new())
            },
        ];

        assert_eq!(locate(&series, 0), Some((0, 0)));
        assert_eq!(locate(&series, 1), Some((0, 1)));
        assert_eq!(locate(&series, 2), Some((1, 0)));
        assert_eq!(locate(&series, 3), None);
    }
}
