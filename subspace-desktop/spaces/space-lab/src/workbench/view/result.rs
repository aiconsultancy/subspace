//! The result panel: chart/data/recipe tabs over the last run (or the
//! static snapshot replayed from a saved preset).

use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::charts::{ChartKind, ChartModel, Series};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::chart::Chart;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabSpec};

use crate::contracts::{Dataset, RunResult};

use super::super::Workbench;
use super::super::state::ResultTab;

pub(super) fn chart_kind(view: &str) -> ChartKind {
    serde_json::from_value::<ChartKind>(serde_json::Value::String(view.into()))
        .ok()
        .filter(|kind| matches!(kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area))
        .unwrap_or_default()
}

pub(super) fn chart_model(dataset: &Dataset, view: &str) -> ChartModel {
    ChartModel {
        kind: chart_kind(view),
        title: "Grouped totals".into(),
        subtitle: if dataset.provenance.contains("synthetic-fixture") {
            "Synthetic fixture data, not live service data"
        } else {
            "Managed recipe data · inspect provenance"
        }
        .into(),
        labels: dataset.labels.clone(),
        legend: dataset.series.len() > 1,
        series: dataset
            .series
            .iter()
            .map(|series| {
                Series::new(
                    series.name.clone(),
                    series.values.iter().map(|value| Some(*value)).collect(),
                )
            })
            .collect(),
        ..ChartModel::default()
    }
}

pub(super) fn result_panel(
    this: &Workbench,
    _window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> gpui_kit::Div {
    let result_tabs = result_tabs(this, cx);
    let body = result_body(this, t);
    v_flex()
        .p(t.pad_x)
        .gap_2()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(result_tabs)
        .child(body)
}

fn result_tabs(this: &Workbench, cx: &mut Context<Workbench>) -> impl IntoElement {
    dashboard_tabs::tab_bar("workbench-result-tabs")
        .selected_index(this.view.result_tab.index())
        .children([
            dashboard_tabs::tab(TabSpec::new("Chart", IconName::ChartColumn)),
            dashboard_tabs::tab(TabSpec::new("Data", IconName::Table)),
            dashboard_tabs::tab(TabSpec::new("Recipe / receipt", IconName::Braces)),
        ])
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            this.view.result_tab = ResultTab::at(*index);
            cx.notify();
        }))
}

fn result_body(this: &Workbench, t: &Tokens) -> gpui_kit::AnyElement {
    if this.view.result_tab == ResultTab::Recipe {
        return recipe_body(this, t);
    }
    match &this.last_result {
        Some(result) if this.view.result_tab == ResultTab::Data => data_body(result, t),
        Some(result) => chart_body(result, &this.draft.view, t),
        None if this.jobs.last_error.is_some() => error_body(this, t),
        None => div()
            .text_color(t.muted)
            .child("No run yet. Press Run to execute the bound recipe through the managed environment.")
            .into_any_element(),
    }
}

fn recipe_body(this: &Workbench, t: &Tokens) -> gpui_kit::AnyElement {
    let source = this.recipe_source.as_str();
    v_flex()
        .id("workbench-recipe-source")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_1()
        .font_family("monospace")
        .text_size(t.mono_font)
        .child(div().text_color(t.cyan).child(format!(
            "Recipe {} · environment {}",
            this.draft.recipe, this.draft.environment
        )))
        .children(this.last_result.as_ref().map(|result| receipt(result, t)))
        .children(source.lines().map(|line| div().child(line.to_owned())))
        .into_any_element()
}

fn data_body(result: &RunResult, t: &Tokens) -> gpui_kit::AnyElement {
    v_flex()
        .id("workbench-full-data")
        .overflow_y_scroll()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .text_size(t.secondary_size())
                .child(result.dataset.summary.clone()),
        )
        .child(
            div()
                .text_size(t.mono_font)
                .font_family("monospace")
                .child(result.dataset.columns.join(" | ")),
        )
        .children(result.dataset.rows.iter().map(|row| {
            div()
                .text_size(t.mono_font)
                .font_family("monospace")
                .child(row.to_string())
        }))
        .into_any_element()
}

fn chart_body(result: &RunResult, view: &str, t: &Tokens) -> gpui_kit::AnyElement {
    let model = Rc::new(chart_model(&result.dataset, view));
    v_flex()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(Chart::new("workbench-chart", model).fill())
        .child(
            div()
                .text_size(t.caption_size())
                .text_color(t.muted)
                .child(format!(
                    "{} groups · controls apply on Run · saved results are snapshots",
                    result.dataset.rows.len()
                )),
        )
        .into_any_element()
}

fn error_body(this: &Workbench, t: &Tokens) -> gpui_kit::AnyElement {
    v_flex()
        .id("workbench-run-error")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_1()
        .text_size(t.mono_font)
        .font_family("monospace")
        .text_color(t.red)
        .children(
            this.jobs
                .last_error
                .as_deref()
                .unwrap_or_default()
                .lines()
                .map(|line| div().child(line.to_owned())),
        )
        .into_any_element()
}

fn receipt(result: &RunResult, t: &Tokens) -> impl IntoElement {
    v_flex()
        .gap_1()
        .pt_2()
        .border_t_1()
        .border_color(t.hair)
        .child(
            div()
                .text_size(t.caption_size())
                .text_color(t.cyan)
                .child(format!(
                    "Source: {} ({} ms)",
                    result.runtime, result.elapsed_ms
                )),
        )
        .child(
            div()
                .text_size(t.caption_size())
                .text_color(t.muted)
                .child(format!("Provenance: {}", result.dataset.provenance)),
        )
}
