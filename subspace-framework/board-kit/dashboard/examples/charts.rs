//! Every chart kind in one window, to check `chart::Chart` by eye:
//! `cargo run -p subspace-dashboard --example charts [light|dark]`.
//!
//! The window opens in One Dark. `light` and `dark` apply the Solarized theme's
//! two variants, so the same charts can be read in all three.

use std::collections::BTreeMap;

use gpui_kit::component::{Theme, ThemeSet, h_flex, v_flex};
use gpui_kit::{
    App, AppContext, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use subspace_charts::{AxisScale, ChartKind, ChartModel, Orientation, Point, Series, ValueFormat};
use subspace_dashboard::Tokens;
use subspace_dashboard::chart::Chart;
use subspace_dashboard::tokens::ThemeTones;

const ONE_DARK_THEME: &str = include_str!("themes/one-dark.json");
const SOLARIZED_THEME: &str = include_str!("themes/solarized.json");
const WINDOW: (f32, f32) = (1420., 1000.);
/// Cards per row, and the height each chart is given.
const COLUMNS: usize = 3;
const CHART: f32 = 185.;
const SAMPLE_DAYS: usize = 14;
/// The growth multiplier doubles after this many days.
const SAMPLE_GROWTH_DOUBLING_DAYS: f64 = 40.;

/// A day of October 2026 as milliseconds since the epoch.
// qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
fn day(index: i64) -> f64 {
    (1_790_812_800_000 + index * 86_400_000) as f64
}

fn usd() -> ValueFormat {
    ValueFormat::Currency { code: "USD".into() }
}

// qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
fn daily() -> ChartModel {
    ChartModel {
        kind: ChartKind::Bar,
        stacked: true,
        title: "Platform carries most of the daily cost".into(),
        subtitle: "All environments, USD per day".into(),
        labels: (1..=14).map(|day| format!("{day:02} Oct")).collect(),
        category_axis_title: "date".into(),
        value_axis_title: "cost".into(),
        series: vec![
            series("platform", &[320., 318., 341., 352., 330.]),
            series("data", &[210., 205., 230., 199., 240.]),
            series("web", &[120., 132., 118., 141., 126.]),
            series("api", &[64., 61., 70., 58., 66.]),
            series("ml", &[22., 31., 18., 44., 27.]),
        ],
        legend: true,
        format: usd(),
        ..ChartModel::default()
    }
}

/// A series whose values repeat `pattern` across fourteen days.
// qual:allow(complexity, magic_numbers) reason: "1 is the unit multiplier before the sample growth is applied."
fn series(label: &str, pattern: &[f64]) -> Series {
    Series::new(
        label,
        (0..SAMPLE_DAYS)
            .map(|index| {
                Some(
                    pattern[index % pattern.len()]
                        * (1. + index as f64 / SAMPLE_GROWTH_DOUBLING_DAYS),
                )
            })
            .collect(),
    )
}

fn totals(orientation: Orientation) -> ChartModel {
    ChartModel {
        kind: ChartKind::Bar,
        orientation,
        title: match orientation {
            Orientation::Vertical => "Cost by team".into(),
            Orientation::Horizontal => "Cost by environment".into(),
        },
        labels: match orientation {
            Orientation::Vertical => ["platform", "data", "web", "api", "ml"]
                .map(String::from)
                .to_vec(),
            Orientation::Horizontal => ["Production", "Staging", "Development"]
                .map(String::from)
                .to_vec(),
        },
        series: match orientation {
            Orientation::Vertical => vec![Series::new(
                "cost",
                [4820., 3110., 1840., 940., 410.].map(Some).to_vec(),
            )],
            Orientation::Horizontal => {
                vec![Series::new(
                    "cost",
                    [6420., 3180., 1520.].map(Some).to_vec(),
                )]
            }
        },
        format: usd(),
        ..ChartModel::default()
    }
}

fn grouped() -> ChartModel {
    ChartModel {
        kind: ChartKind::Bar,
        title: "Cost by environment and team".into(),
        labels: ["dev", "staging", "prod"].map(String::from).to_vec(),
        series: vec![
            Series::new("platform", [310., 820., 3690.].map(Some).to_vec()),
            Series::new("data", [180., 640., 2290.].map(Some).to_vec()),
            Series::new("web", [240., 510., 1090.].map(Some).to_vec()),
        ],
        legend: true,
        format: usd(),
        ..ChartModel::default()
    }
}

// qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
fn line(kind: ChartKind) -> ChartModel {
    let shape = |offset: f64, scale: f64| {
        (0..SAMPLE_DAYS)
            .map(|index| Point {
                x: day(index as i64),
                y: offset + scale * (index as f64 * 0.7).sin().abs(),
            })
            .collect::<Vec<_>>()
    };
    ChartModel {
        kind,
        stacked: kind == ChartKind::Area,
        title: match kind {
            ChartKind::Area => "Daily cost, stacked by team".into(),
            _ => "Daily cost per team".into(),
        },
        series: vec![
            points("platform", shape(300., 60.)),
            points("data", shape(190., 50.)),
            points("web", shape(110., 40.)),
        ],
        x_scale: AxisScale::Timestamp,
        legend: true,
        format: usd(),
        ..ChartModel::default()
    }
}

fn points(label: &str, points: Vec<Point>) -> Series {
    Series {
        points,
        ..Series::new(label, Vec::new())
    }
}

// qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
fn slices(kind: ChartKind) -> ChartModel {
    let services = [
        ("Kubernetes", 4820.),
        ("Virtual Machines", 2110.),
        ("Log Analytics", 1240.),
        ("Storage", 640.),
        ("Bastion", 410.),
        ("Container Apps", 180.),
    ];
    ChartModel {
        kind,
        title: match kind {
            ChartKind::Pie => "Top services".into(),
            _ => "Top services, month to date".into(),
        },
        labels: services
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect(),
        series: services
            .iter()
            .map(|(name, cost)| Series::new(*name, vec![Some(*cost)]))
            .collect(),
        legend: true,
        format: usd(),
        ..ChartModel::default()
    }
}

// qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
fn scatter() -> ChartModel {
    let cloud = |seed: f64| {
        (0..24)
            .map(|index| {
                let step = index as f64;
                Point {
                    x: 20. + step * 7. + (step * seed).sin() * 18.,
                    y: 40. + step * 11. + (step * seed * 1.7).cos() * 60.,
                }
            })
            .collect::<Vec<_>>()
    };
    ChartModel {
        kind: ChartKind::Scatter,
        title: "Cost against node hours".into(),
        subtitle: "one mark per service".into(),
        series: vec![points("staging", cloud(1.1)), points("prod", cloud(2.3))],
        legend: true,
        format: usd(),
        ..ChartModel::default()
    }
}

struct Gallery;

impl Render for Gallery {
    // qual:allow(complexity, magic_numbers) reason: "Sample cost data for the eye-check gallery. The numbers are the data, not settings; naming each one would say nothing the series label does not."
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let cards = [
            ("daily", daily()),
            ("totals", totals(Orientation::Vertical)),
            ("environments", totals(Orientation::Horizontal)),
            ("grouped", grouped()),
            ("line", line(ChartKind::Line)),
            ("area", line(ChartKind::Area)),
            ("pie", slices(ChartKind::Pie)),
            ("doughnut", slices(ChartKind::Doughnut)),
            ("scatter", scatter()),
            ("empty", ChartModel::default()),
        ];
        let mut rows = v_flex().gap_3();
        for pair in cards.chunks(COLUMNS) {
            let mut row = h_flex().gap_3().items_start();
            for (name, model) in pair {
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .p_3()
                        .rounded(px(6.))
                        .bg(tokens.pane)
                        .border_1()
                        .border_color(tokens.line)
                        .child(Chart::new(*name, model.clone()).height(px(CHART))),
                );
            }
            rows = rows.child(row);
        }
        div()
            .id("page")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .text_size(tokens.row_font)
            .child(rows)
    }
}

/// Apply a bundled Subspace theme and its dashboard tones, the way the app's
/// own theme loader does, so the charts show their real colours.
fn apply_theme(source: &str, mode: &str, cx: &mut App) {
    let Ok(set) = serde_json::from_str::<ThemeSet>(source) else {
        eprintln!("the theme could not be read");
        return;
    };
    let Some(config) = set
        .themes
        .into_iter()
        .find(|theme| theme.mode.name() == mode)
    else {
        eprintln!("the theme has no {mode} variant");
        return;
    };
    cx.set_global(ThemeTones(tones(source, &config.name)));
    let config = std::rc::Rc::new(config);
    Theme::update(cx, |theme| theme.apply_config(&config));
}

/// The `subspace.tones` of one theme variant, which override the dashboard
/// colours the chart palette is built from.
fn tones(source: &str, name: &str) -> BTreeMap<String, Hsla> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(source) else {
        return BTreeMap::new();
    };
    let Some(themes) = document["themes"].as_array() else {
        return BTreeMap::new();
    };
    themes
        .iter()
        .find(|theme| theme["name"] == name)
        .and_then(|theme| theme["subspace"]["tones"].as_object())
        .map(|tones| {
            tones
                .iter()
                .filter_map(|(tone, value)| Some((tone.clone(), hex(value.as_str()?)?)))
                .collect()
        })
        .unwrap_or_default()
}

/// A `#rrggbb` string as a colour.
fn hex(value: &str) -> Option<Hsla> {
    let digits = value.strip_prefix('#')?;
    u32::from_str_radix(digits, 16)
        .ok()
        .map(|color| Hsla::from(rgb(color)))
}

fn main() {
    let mode = std::env::args().nth(1);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            match mode.as_deref() {
                Some("light") => apply_theme(SOLARIZED_THEME, "light", cx),
                Some("dark") => apply_theme(SOLARIZED_THEME, "dark", cx),
                _ => apply_theme(ONE_DARK_THEME, "dark", cx),
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    None,
                    size(px(WINDOW.0), px(WINDOW.1)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Gallery)) {
                eprintln!("charts could not open its window: {error}");
                cx.quit();
            }
        });
}
