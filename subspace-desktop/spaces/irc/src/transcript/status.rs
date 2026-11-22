//! The live status line under a request: who works on it and how far they
//! got, from [`crate::thread_status`]. GPUI Kit's `Marker`, the row Kit
//! offers for conversation status, with the shared warning pill while the
//! last update may be out of date.

use chrono::Local;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::marker::{Marker, MarkerContent, MarkerIcon};
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement as _, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::badge::{Pill, PillTone};
use subspace_framework::dashboard::panel::IconName;

use crate::state::Entry;
use crate::thread_status::{Shown, Statuses};

/// The status line under `entry`, aligned with the message body, when it is
/// a request still in progress. Read-only: no focus or action.
pub(super) fn line(entry: &Entry, statuses: &Statuses, tokens: &Tokens) -> Option<AnyElement> {
    let shown = statuses.get(&entry.seq)?;
    let request = entry.line.id.as_deref()?;
    Some(status_line(request, shown, tokens))
}

/// One request's line, such as `astra · Using a tool`, updated in place.
fn status_line(request: &str, status: &Shown, tokens: &Tokens) -> AnyElement {
    let id = format!("irc-status:{request}");
    let since = status
        .stale
        .map(|time| time.with_timezone(&Local).format("%H:%M").to_string());
    let text = format!("{} · {}", status.agent, status.label);
    div()
        .id(SharedString::from(id.clone()))
        .role(Role::Status)
        .aria_label(spoken(status, since.as_deref()))
        .test_support()
        .py_1()
        .child(
            Marker::new()
                .text_size(tokens.caption_size())
                .icon(MarkerIcon::new().child(Icon::from(glyph(since.is_some())).xsmall()))
                .content(MarkerContent::new().text(text))
                .children(since.map(|at| stale(&id, &at))),
        )
        .into_any_element()
}

/// What assistive technology reads, with when the line may be out of date.
fn spoken(status: &Shown, since: Option<&str>) -> String {
    let line = format!("{}: {}", status.agent, status.label);
    match since {
        Some(at) => format!("{line}. Stale: the last update was at {at}"),
        None => line,
    }
}

/// Decorative: the text says the same.
fn glyph(stale: bool) -> IconName {
    if stale {
        IconName::Unplug
    } else {
        IconName::Bot
    }
}

/// When the last update came, and the shared warning pill.
fn stale(id: &str, at: &str) -> AnyElement {
    h_flex()
        .gap_2()
        .child(format!("Last update {at}"))
        .child(
            Pill::new(SharedString::from(format!("{id}:stale")), "Stale")
                .tone(PillTone::Warning)
                .aria_label(format!("Stale: the last update was at {at}")),
        )
        .into_any_element()
}
