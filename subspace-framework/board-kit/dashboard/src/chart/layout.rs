//! The arithmetic behind a chart: the extent its values span, the ticks on
//! its value axis, the band each category takes, which labels still fit, and
//! how a value reads.
//!
//! All of it is pure, so it is tested without a window.

use subspace_charts::{AxisScale, ChartModel, Series, ValueFormat};

/// How much of a category's band is gap rather than mark.
const BAND_PADDING: f32 = 0.3;
/// How much of one series' slot inside a band is gap.
const SLOT_PADDING: f32 = 0.12;
/// The ticks a value axis aims for.
pub const TICK_COUNT: usize = 5;
/// How far outside its axis a value may be placed, so a wild number cannot
/// overflow the geometry.
const FAR: f32 = 1e4;
/// Where a tick gains a thousands, millions or billions suffix. Values below
/// ten thousand read better in full.
const BILLION: f64 = 1e9;
const MILLION: f64 = 1e6;
const THOUSAND: f64 = 1e3;
const COMPACT_FLOOR: f64 = 1e4;
/// The decimals a value and a share keep.
const VALUE_PLACES: usize = 2;
const SHARE_PLACES: usize = 1;

/// The extent the value axis spans. It always includes zero, so a bar's
/// length is its value, and it never collapses to a single point.
pub fn value_extent(model: &ChartModel) -> (f64, f64) {
    let (low, high) = match (model.stacked, model.is_point_chart()) {
        (true, true) => point_stack_extent(&model.series),
        (true, false) => stacked_extent(&model.series, model.labels.len()),
        _ => plain_extent(&model.series),
    };
    widen(low, high)
}

/// The extent of series drawn side by side, which is their own extent.
fn plain_extent(series: &[Series]) -> (f64, f64) {
    series.iter().fold((0., 0.), |(low, high), one| {
        let (min, max) = (one.min(), one.max());
        (
            if min.is_finite() { low.min(min) } else { low },
            if max.is_finite() { high.max(max) } else { high },
        )
    })
}

/// The extent of stacked series, which is the extent of their running totals.
pub fn stacked_extent(series: &[Series], categories: usize) -> (f64, f64) {
    (0..categories).fold((0., 0.), |(low, high), category| {
        let (negative, positive) = totals(series, category);
        (low.min(negative), high.max(positive))
    })
}

/// The extent of stacked point series: their totals at every x they share.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
fn point_stack_extent(series: &[Series]) -> (f64, f64) {
    x_positions(series)
        .into_iter()
        .fold((0., 0.), |(low, high), x| {
            let total = running_at(series, series.len(), x);
            (low.min(total), high.max(total))
        })
}

/// Every x a point series has, in order and without repeats.
pub fn x_positions(series: &[Series]) -> Vec<f64> {
    let mut positions: Vec<f64> = series
        .iter()
        .flat_map(|one| one.points.iter().map(|point| point.x))
        .collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup();
    positions
}

/// The total of the first `count` series at `x`, counting only the series
/// that have a point there.
pub fn running_at(series: &[Series], count: usize, x: f64) -> f64 {
    series
        .iter()
        .take(count)
        .filter_map(|one| y_at(one, x))
        .sum()
}

/// One series' y at `x`, when it has a point there.
pub fn y_at(series: &Series, x: f64) -> Option<f64> {
    series
        .points
        .iter()
        .find(|point| point.x == x)
        .map(|point| point.y)
}

/// The negative and positive totals of one category.
fn totals(series: &[Series], category: usize) -> (f64, f64) {
    series.iter().fold((0., 0.), |(negative, positive), one| {
        match value(one, category) {
            Some(value) if value < 0. => (negative + value, positive),
            Some(value) => (negative, positive + value),
            None => (negative, positive),
        }
    })
}

/// One series' value for a category, when it has one.
pub fn value(series: &Series, category: usize) -> Option<f64> {
    series
        .values
        .get(category)
        .copied()
        .flatten()
        .filter(|value| value.is_finite())
}

/// An extent with no span cannot place a value, so it gains one.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
fn widen(low: f64, high: f64) -> (f64, f64) {
    match high - low {
        span if span > 0. => (low, high),
        _ => (low.min(0.), low.max(0.) + 1.),
    }
}

/// The base and the top of each series' segment in one stacked category.
/// A series with no value for the category gets an empty segment.
pub fn stack(series: &[Series], category: usize) -> Vec<(f64, f64)> {
    let mut negative = 0.;
    let mut positive = 0.;
    series
        .iter()
        .map(|one| match value(one, category) {
            Some(value) if value < 0. => {
                let top = negative;
                negative += value;
                (negative, top)
            }
            Some(value) => {
                let base = positive;
                positive += value;
                (base, positive)
            }
            None => (0., 0.),
        })
        .collect()
}

/// Round tick values across `extent`, stepping by 1, 2 or 5 times a power of
/// ten so the labels read as whole numbers.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn ticks(extent: (f64, f64), count: usize) -> Vec<f64> {
    let (low, high) = extent;
    let step = nice_step((high - low) / count.max(2) as f64);
    let mut values = Vec::new();
    let mut value = (low / step).floor() * step;
    while value <= high + step / 1e6 && values.len() < count * 3 {
        values.push(tidy(value, step));
        value += step;
    }
    values
}

/// The round step nearest `raw`: 1, 2 or 5 times a power of ten.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0. {
        return 1.;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    let factor = match raw / magnitude {
        normalized if normalized <= 1. => 1.,
        normalized if normalized <= 2. => 2.,
        normalized if normalized <= 5. => 5.,
        _ => 10.,
    };
    factor * magnitude
}

/// Drop the floating-point tail a repeated addition leaves behind.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
fn tidy(value: f64, step: f64) -> f64 {
    let places = (-step.log10().floor()).clamp(0., 9.) as i32 + 2;
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

/// Where `value` sits on an axis that runs from `extent.0` at `from` pixels to
/// `extent.1` at `to` pixels.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn place(value: f64, extent: (f64, f64), from: f32, to: f32) -> f32 {
    let (low, high) = extent;
    let span = high - low;
    if span <= 0. {
        return from;
    }
    let ratio = ((value - low) / span) as f32;
    from + (to - from) * ratio.clamp(-FAR, FAR)
}

/// The band category `index` of `count` takes along `length` pixels, as its
/// start and its width.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn band(index: usize, count: usize, length: f32) -> (f32, f32) {
    if count == 0 {
        return (0., length.max(1.));
    }
    let step = length / count as f32;
    let width = (step * (1. - BAND_PADDING)).max(1.);
    (step * index as f32 + (step - width) / 2., width)
}

/// The slot series `index` of `count` takes inside a band, for grouped marks.
/// One series fills its band.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn slot(index: usize, count: usize, start: f32, width: f32) -> (f32, f32) {
    if count <= 1 {
        return (start, width.max(1.));
    }
    let step = width / count as f32;
    let slot = (step * (1. - SLOT_PADDING)).max(1.);
    (start + step * index as f32 + (step - slot) / 2., slot)
}

/// The centre of a band, which a line point and a tick label sit on.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn centre(index: usize, count: usize, length: f32) -> f32 {
    let (start, width) = band(index, count, length);
    start + width / 2.
}

/// Draw every nth label, so `count` labels needing `room` pixels each still
/// fit along `length`.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
pub fn label_stride(count: usize, length: f32, room: f32) -> usize {
    if count == 0 || length <= 0. {
        return 1;
    }
    let fits = (length / room.max(1.)).floor().max(1.) as usize;
    count.div_ceil(fits).max(1)
}

/// A value as a reader sees it in a tooltip or a legend.
pub fn format_value(value: f64, format: &ValueFormat) -> String {
    match format {
        ValueFormat::Plain => decimal(value, VALUE_PLACES),
        ValueFormat::Currency { code } => money(value, code, decimal(value.abs(), VALUE_PLACES)),
        ValueFormat::Percent => format!("{}%", decimal(value, SHARE_PLACES)),
    }
}

/// A value short enough for an axis tick.
pub fn format_tick(value: f64, format: &ValueFormat) -> String {
    match format {
        ValueFormat::Plain => compact(value),
        ValueFormat::Currency { code } => money(value, code, compact(value.abs())),
        ValueFormat::Percent => format!("{}%", compact(value)),
    }
}

/// An amount with its currency symbol, with the sign ahead of the symbol.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
fn money(value: f64, code: &str, digits: String) -> String {
    let sign = match value < 0. {
        true => "-",
        false => "",
    };
    format!("{sign}{}{digits}", symbol(code))
}

/// An x position of a point chart, as an axis tick.
pub fn format_x(value: f64, scale: AxisScale) -> String {
    match scale {
        AxisScale::Number => compact(value),
        AxisScale::Timestamp => stamp(value, "%d %b"),
    }
}

/// The same position in full, for a tooltip.
pub fn format_x_full(value: f64, scale: AxisScale) -> String {
    match scale {
        AxisScale::Number => decimal(value, VALUE_PLACES),
        AxisScale::Timestamp => stamp(value, "%Y-%m-%d"),
    }
}

/// A currency's symbol, or its code with a space when there is none.
fn symbol(code: &str) -> String {
    match code {
        "USD" => "$".to_owned(),
        "EUR" => "€".to_owned(),
        "GBP" => "£".to_owned(),
        "" => String::new(),
        other => format!("{other} "),
    }
}

/// A number with thousands separators and at most `places` decimals, with a
/// trailing zero dropped.
fn decimal(value: f64, places: usize) -> String {
    if !value.is_finite() {
        return "—".to_owned();
    }
    let rounded = format!("{value:.places$}");
    let (sign, digits) = match rounded.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", rounded.as_str()),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let fraction = fraction.trim_end_matches('0');
    match fraction.is_empty() {
        true => format!("{sign}{}", group(whole)),
        false => format!("{sign}{}.{fraction}", group(whole)),
    }
}

/// Thousands separators for a run of digits.
// qual:allow(complexity, magic_numbers) reason: "Chart arithmetic: the numbers are the formula itself - a zero base, the 1-2-5 tick ladder, a half of a band. The paddings, tick count and scale thresholds that are choices are named constants."
fn group(whole: &str) -> String {
    let mut grouped = String::with_capacity(whole.len() + whole.len() / 3);
    for (offset, digit) in whole.char_indices() {
        if offset > 0 && (whole.len() - offset).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A number short enough for a tick: thousands and above carry a suffix.
fn compact(value: f64) -> String {
    let magnitude = value.abs();
    let (scaled, suffix) = match magnitude {
        _ if magnitude >= BILLION => (value / BILLION, "B"),
        _ if magnitude >= MILLION => (value / MILLION, "M"),
        _ if magnitude >= COMPACT_FLOOR => (value / THOUSAND, "k"),
        _ => (value, ""),
    };
    let places = match suffix.is_empty() {
        true => VALUE_PLACES,
        false => SHARE_PLACES,
    };
    format!("{}{suffix}", decimal(scaled, places))
}

/// Milliseconds since the epoch, written in UTC so a day keeps its date.
fn stamp(millis: f64, pattern: &str) -> String {
    match chrono::DateTime::from_timestamp_millis(millis as i64) {
        Some(moment) => moment.format(pattern).to_string(),
        None => decimal(millis, 0),
    }
}

#[cfg(test)]
mod tests {
    use crate::chart::layout::{
        band, centre, format_tick, format_value, format_x, format_x_full, label_stride, nice_step,
        place, running_at, slot, stack, stacked_extent, ticks, value_extent, x_positions, y_at,
    };
    use subspace_charts::{
        AxisScale, ChartKind, ChartModel, Point as DataPoint, Series, ValueFormat,
    };

    fn model(stacked: bool) -> ChartModel {
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
    fn a_stacked_extent_reaches_the_tallest_total_not_the_tallest_series() {
        assert_eq!(value_extent(&model(false)), (0., 30.));
        assert_eq!(value_extent(&model(true)), (0., 50.));

        let signed = ChartModel {
            stacked: true,
            labels: vec!["Mon".into()],
            series: vec![
                Series::new("gain", vec![Some(4.0)]),
                Series::new("loss", vec![Some(-6.0)]),
            ],
            ..ChartModel::default()
        };
        assert_eq!(value_extent(&signed), (-6., 4.));
        assert_eq!(stacked_extent(&signed.series, 1), (-6., 4.));
    }

    #[test]
    fn an_empty_chart_still_gets_an_axis_to_place_values_on() {
        assert_eq!(value_extent(&ChartModel::default()), (0., 1.));
        assert_eq!(place(0.5, (0., 1.), 100., 0.), 50.);
        assert_eq!(place(1.0, (0., 1.), 100., 100.), 100.);
    }

    #[test]
    fn a_stack_gives_each_series_the_segment_above_the_one_before() {
        let series = model(true).series;
        assert_eq!(stack(&series, 0), [(0., 30.), (30., 50.)]);
        assert_eq!(
            stack(&series, 1),
            [(0., 10.), (0., 0.)],
            "a gap takes no room in the stack"
        );

        let signed = [
            Series::new("gain", vec![Some(4.0)]),
            Series::new("loss", vec![Some(-6.0)]),
        ];
        assert_eq!(stack(&signed, 0), [(0., 4.), (-6., 0.)]);
    }

    #[test]
    fn stacked_point_series_total_at_every_x_they_share() {
        let series = |label: &str, ys: [f64; 2]| Series {
            points: vec![
                DataPoint { x: 10., y: ys[0] },
                DataPoint { x: 20., y: ys[1] },
            ],
            ..Series::new(label, Vec::new())
        };
        let model = ChartModel {
            kind: ChartKind::Area,
            stacked: true,
            series: vec![series("platform", [30., 10.]), series("pmr", [20., 50.])],
            ..ChartModel::default()
        };

        assert_eq!(x_positions(&model.series), [10., 20.]);
        assert_eq!(running_at(&model.series, 1, 10.), 30.);
        assert_eq!(running_at(&model.series, 2, 10.), 50.);
        assert_eq!(running_at(&model.series, 2, 20.), 60.);
        assert_eq!(y_at(&model.series[1], 20.), Some(50.));
        assert_eq!(y_at(&model.series[1], 99.), None);
        assert_eq!(
            value_extent(&model),
            (0., 60.),
            "the axis reaches the tallest total"
        );
    }

    #[test]
    fn ticks_step_by_one_two_or_five_times_a_power_of_ten() {
        assert_eq!(nice_step(0.7), 1.);
        assert_eq!(nice_step(3.), 5.);
        assert_eq!(nice_step(21.), 50.);
        assert_eq!(nice_step(0.), 1.);
        assert_eq!(nice_step(f64::NAN), 1.);

        assert_eq!(ticks((0., 50.), 5), [0., 10., 20., 30., 40., 50.]);
        assert_eq!(ticks((0., 1.), 5), [0., 0.2, 0.4, 0.6, 0.8, 1.]);
        assert_eq!(ticks((-6., 4.), 5), [-6., -4., -2., 0., 2., 4.]);
        assert!(ticks((0., 1e9), 5).len() <= 15);
    }

    #[test]
    fn a_band_keeps_a_gap_and_a_slot_splits_it_between_the_series() {
        let (start, width) = band(0, 2, 100.);
        assert_eq!((start, width), (7.5, 35.));
        assert_eq!(band(1, 2, 100.), (57.5, 35.));
        assert_eq!(centre(0, 2, 100.), 25.);
        assert_eq!(centre(1, 2, 100.), 75.);
        assert_eq!(band(0, 0, 100.), (0., 100.), "no categories, one band");

        assert_eq!(slot(0, 1, start, width), (start, width));
        let (first, slot_width) = slot(0, 2, 0., 100.);
        let (second, _) = slot(1, 2, 0., 100.);
        assert!(slot_width < 50., "two slots leave a gap");
        assert!(first < second && second + slot_width <= 100.);
    }

    #[test]
    fn crowded_labels_are_thinned_rather_than_overlapped() {
        assert_eq!(label_stride(5, 500., 50.), 1);
        assert_eq!(label_stride(30, 300., 50.), 5);
        assert_eq!(label_stride(30, 10., 50.), 30);
        assert_eq!(label_stride(0, 300., 50.), 1);
        assert_eq!(label_stride(30, 0., 50.), 1);
    }

    #[test]
    fn values_read_as_money_shares_or_plain_numbers() {
        let usd = ValueFormat::Currency { code: "USD".into() };
        assert_eq!(format_value(1234.5, &usd), "$1,234.5");
        assert_eq!(format_value(1234.567, &usd), "$1,234.57");
        assert_eq!(format_value(-99.0, &usd), "-$99");
        assert_eq!(format_value(12.25, &ValueFormat::Plain), "12.25");
        assert_eq!(format_value(42.0, &ValueFormat::Percent), "42%");
        assert_eq!(
            format_value(7.0, &ValueFormat::Currency { code: "SEK".into() }),
            "SEK 7"
        );

        assert_eq!(format_tick(1500.0, &usd), "$1,500");
        assert_eq!(format_tick(12500.0, &usd), "$12.5k");
        assert_eq!(format_tick(3_400_000.0, &ValueFormat::Plain), "3.4M");
        assert_eq!(format_tick(0.0, &ValueFormat::Plain), "0");
    }

    #[test]
    fn a_timestamp_axis_reads_as_a_date_not_a_number() {
        let day = 1_790_812_800_000.0;
        assert_eq!(format_x(day, AxisScale::Timestamp), "01 Oct");
        assert_eq!(format_x_full(day, AxisScale::Timestamp), "2026-10-01");
        assert_eq!(format_x(12_500.0, AxisScale::Number), "12.5k");
        assert_eq!(format_x_full(12_500.5, AxisScale::Number), "12,500.5");
    }
}
