//! Lists and tables: the native grid and charts.
//!
//! `panel::frame` is not here: its resize handle saves the app-wide panel
//! width, and the gallery must not write preferences.

pub(crate) mod fixture;

use gpui_kit::component::v_flex;
use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div, px};
use subspace_charts::{ChartKind, ChartModel, Series};
use subspace_dashboard::chart::Chart;
use subspace_dashboard::{Tokens, list};

use super::{Sample, bounded, sample};
use crate::ComponentGallery;
use crate::catalog::Item;

const GRID_BOX: (f32, f32) = (420., 160.);
const LIST_BOX: (f32, f32) = (420., 260.);
const NOTES_BOX: (f32, f32) = (420., 240.);
const CHART_BOX: (f32, f32) = (420., 220.);
const CHART_HEIGHT: f32 = 200.;
const BUILDS: [f64; 3] = [4., 9., 6.];

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "list::render",
        source: "board-kit/dashboard/src/list/view.rs",
        used_in: "The heading and context used by Issues, PRs and Actions, and grouped rows. Select a row, fold a group or use the arrow keys. Actions adds a note while a repository loads or when its filter matches nothing.",
        samples: list_render,
        kit: None,
    },
    Item {
        name: "native_grid::NativeGrid",
        source: "board-kit/dashboard/src/native_grid.rs",
        used_in: "Tabular results in the Space Lab project view.",
        samples: native_grid,
        kit: None,
    },
    Item {
        name: "chart::Chart",
        source: "board-kit/dashboard/src/chart.rs",
        used_in: "Result charts in Space Lab and chat canvases.",
        samples: chart,
        kit: None,
    },
];

pub(super) fn native_grid(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    gallery
        .retained
        .grid
        .iter()
        .map(|grid| sample("3 × 3 fixture", bounded(GRID_BOX, tokens, grid.clone())))
        .collect()
}

pub(super) fn bar_model() -> ChartModel {
    ChartModel {
        kind: ChartKind::Bar,
        title: "Fixture builds per day".into(),
        labels: ["Mon", "Tue", "Wed"].map(str::to_owned).to_vec(),
        category_axis_title: "day".into(),
        value_axis_title: "builds".into(),
        series: vec![Series::new("builds", BUILDS.map(Some).to_vec())],
        ..ChartModel::default()
    }
}

pub(super) fn chart(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let chart = Chart::new("gallery-sample:chart", bar_model()).height(px(CHART_HEIGHT));
    vec![sample("Bar", bounded(CHART_BOX, tokens, chart))]
}

pub(super) fn list_render(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let heading = list::header(
        "gallery-list",
        "Pull requests",
        "GitHub · Signed in as octo-dev",
        tokens.pad_x,
        cx,
    );
    let list = v_flex().size_full().child(heading).child(
        div()
            .flex_1()
            .min_h_0()
            .child(gallery.retained.list.clone()),
    );
    vec![
        sample(
            "Heading, account context and grouped rows",
            bounded(LIST_BOX, tokens, list),
        ),
        sample(
            "Group notes: loading and empty",
            bounded(NOTES_BOX, tokens, gallery.retained.list_notes.clone()),
        ),
    ]
}
