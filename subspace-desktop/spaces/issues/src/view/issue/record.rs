use super::panel::{self, Fact};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, TestSupportExt as _, div,
};
use subspace_api_client::model::IssueDetail;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::workspace;

pub(super) fn facts(detail: &IssueDetail) -> Vec<Fact> {
    let original = detail.fields.provenance.as_ref();
    let dates = [
        (
            "Created",
            Some(detail.fields.created_at.as_str()),
            original.and_then(|p| p.original_created_at.as_deref()),
        ),
        (
            "Started",
            detail.started_at.as_deref(),
            original.and_then(|p| p.original_started_at.as_deref()),
        ),
        (
            "Completed",
            detail.completed_at.as_deref(),
            original.and_then(|p| p.original_completed_at.as_deref()),
        ),
        (
            "Cancelled",
            detail.canceled_at.as_deref(),
            original.and_then(|p| p.original_canceled_at.as_deref()),
        ),
        ("Archived", detail.fields.archived_at.as_deref(), None),
    ];
    let mut facts: Vec<_> = dates
        .into_iter()
        .filter_map(|(label, native, original)| {
            original
                .or(native)
                .map(|date| Fact::new(date_label(label, original.is_some()), format_date(date)))
        })
        .collect();
    facts.extend(
        detail
            .created_by
            .as_ref()
            .map(|p| Fact::new("Created by", super::properties::person(p))),
    );
    facts.extend(
        detail
            .archived_by
            .as_ref()
            .map(|p| Fact::new("Archived by", super::properties::person(p))),
    );
    facts.push(Fact::new("Updated", format_date(&detail.fields.updated_at)));
    facts
}

fn date_label(label: &'static str, original: bool) -> &'static str {
    match (label, original) {
        ("Created", true) => "Originally created",
        ("Started", true) => "Originally started",
        ("Completed", true) => "Originally completed",
        ("Cancelled", true) => "Originally cancelled",
        _ => label,
    }
}

/// `2026-12-17` as `17 Dec 2026`; anything else as it is.
pub(crate) fn format_date(value: &str) -> String {
    crate::view::lanes::short_date(value)
        .and_then(|short| value.get(..4).map(|year| format!("{short} {year}")))
        .unwrap_or_else(|| value.to_owned())
}

/// The record under a quiet caption, in compact fact rows.
pub(super) fn render(detail: &IssueDetail, tokens: &Tokens, cx: &App) -> AnyElement {
    div()
        .id("issue-record")
        .aria_label("Record")
        .test_support()
        .child(
            workspace::section("Record", cx).child(
                panel::grid().children(
                    facts(detail)
                        .into_iter()
                        .map(|fact| panel::fact(fact, "record", tokens)),
                ),
            ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
