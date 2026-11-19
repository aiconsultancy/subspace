//! GPUI Kit charts, one sample per kind, and a small custom `Plot`, over
//! fixture series coloured from the theme.

use gpui_kit::component::chart::{
    AreaChart, BarChart, CandlestickChart, LineChart, PieChart, RadarChart, SankeyChart,
};
use gpui_kit::component::plot::shape::{Line, SankeyLink};
use gpui_kit::component::plot::{Plot, PlotElement};
use gpui_kit::{
    App, Bounds, Context, ElementId, Hsla, IntoElement, ParentElement as _, Pixels, SharedString,
    Window, px,
};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, sample};

const CHART: (f32, f32) = (300., 180.);
const PIE_RADIUS: f32 = 70.;
const DONUT_RADIUS: f32 = 40.;
const RADAR_MAX: f64 = 100.;
const STROKE: f32 = 2.;
const MONTHS: [(&str, f64, f64); 6] = [
    ("Jan", 186., 80.),
    ("Feb", 305., 200.),
    ("Mar", 237., 120.),
    ("Apr", 73., 190.),
    ("May", 209., 130.),
    ("Jun", 214., 140.),
];
const SKILLS: [(&str, f64); 5] = [
    ("Speed", 80.),
    ("Range", 65.),
    ("Grip", 90.),
    ("Weight", 40.),
    ("Cost", 70.),
];
/// Open, high, low and close of five fixture days.
const PRICES: [(&str, f64, f64, f64, f64); 5] = [
    ("Mon", 10., 14., 9., 13.),
    ("Tue", 13., 15., 11., 12.),
    ("Wed", 12., 16., 12., 15.),
    ("Thu", 15., 17., 13., 14.),
    ("Fri", 14., 18., 14., 17.),
];
const FLOW_NODES: [&str; 4] = ["Visits", "Sign-ups", "Trials", "Purchases"];
const FLOW: [(usize, usize, f64); 3] = [(0, 1, 60.), (1, 2, 40.), (2, 3, 25.)];
/// Points of the custom plot, as fractions of its width and height.
const SPARK: [(f32, f32); 6] = [
    (0., 0.8),
    (0.2, 0.4),
    (0.4, 0.6),
    (0.6, 0.2),
    (0.8, 0.5),
    (1., 0.1),
];

pub(crate) fn charts(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let (accent, green) = (tokens.accent, tokens.green);
    let boxed = |chart| bounded(CHART, tokens, chart);
    vec![
        sample(
            "BarChart",
            boxed(
                BarChart::new(MONTHS.to_vec())
                    .band(|d| d.0)
                    .value(|d| d.1)
                    .fill(move |_, _, _, _| accent)
                    .into_any_element(),
            ),
        ),
        sample(
            "LineChart",
            boxed(
                LineChart::new(MONTHS.to_vec())
                    .x(|d| d.0)
                    .y(|d| d.1)
                    .stroke(accent)
                    .dot()
                    .into_any_element(),
            ),
        ),
        sample(
            "AreaChart, two series",
            boxed(
                AreaChart::new(MONTHS.to_vec())
                    .x(|d| d.0)
                    .y(|d| d.1)
                    .stroke(accent)
                    .y(|d| d.2)
                    .stroke(green)
                    .into_any_element(),
            ),
        ),
        sample("PieChart and donut", boxed(pies(tokens))),
        sample(
            "RadarChart",
            boxed(
                RadarChart::new(SKILLS.to_vec())
                    .label(|d| d.0)
                    .value(|d| d.1)
                    .stroke(accent)
                    .max_value(RADAR_MAX)
                    .into_any_element(),
            ),
        ),
        sample(
            "CandlestickChart",
            boxed(
                CandlestickChart::new(PRICES.to_vec())
                    .x(|d| d.0)
                    .open(|d| d.1)
                    .high(|d| d.2)
                    .low(|d| d.3)
                    .close(|d| d.4)
                    .into_any_element(),
            ),
        ),
        sample(
            "SankeyChart",
            boxed(
                SankeyChart::new(
                    FLOW_NODES.to_vec(),
                    FLOW.map(|(from, to, value)| SankeyLink::new(from, to, value)),
                )
                .node_label(|d| SharedString::from(*d))
                .into_any_element(),
            ),
        ),
    ]
}

fn pies(tokens: &Tokens) -> gpui_kit::AnyElement {
    let colours = [
        tokens.accent,
        tokens.green,
        tokens.orange,
        tokens.blue,
        tokens.red,
        tokens.cyan,
    ];
    let fallback = tokens.accent;
    let colour = move |d: &(&str, f64, f64)| {
        let at = MONTHS
            .iter()
            .position(|month| month.0 == d.0)
            .unwrap_or_default();
        colours.get(at).copied().unwrap_or(fallback)
    };
    gpui_kit::component::h_flex()
        .child(
            PieChart::new(MONTHS.to_vec())
                .value(|d| d.1 as f32)
                .outer_radius(PIE_RADIUS)
                .color(colour),
        )
        .child(
            PieChart::new(MONTHS.to_vec())
                .value(|d| d.2 as f32)
                .inner_radius(DONUT_RADIUS)
                .outer_radius(PIE_RADIUS)
                .color(colour),
        )
        .into_any_element()
}

/// A minimal custom plot: a line through fixed fractions of its bounds.
pub(crate) struct Sparkline {
    colour: Hsla,
}

impl IntoElement for Sparkline {
    type Element = PlotElement<Self>;

    fn into_element(self) -> Self::Element {
        PlotElement::new(self)
    }
}

impl Plot for Sparkline {
    fn id(&self) -> Option<ElementId> {
        Some("gallery-kit:plot".into())
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
        let (left, top) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
        let (width, height) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
        Line::new()
            .data(SPARK)
            .x(move |p: &(f32, f32)| Some(left + width * p.0))
            .y(move |p: &(f32, f32)| Some(top + height * p.1))
            .stroke(self.colour)
            .stroke_width(px(STROKE))
            .paint(&bounds, window);
    }
}

pub(crate) fn plots(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "A custom Plot that paints one line",
        bounded(
            CHART,
            tokens,
            Sparkline {
                colour: tokens.accent,
            },
        ),
    )]
}
