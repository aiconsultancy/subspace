//! Board refresh drafts. Opening settings never changes the running policy.

use gpui_kit::{App, Window};
use subspace_dashboard::prefs::{self, Prefs};
use subspace_dashboard::refresh::{self, Preferences};

use super::form::{Choice, choice};

const INTERVALS: &[(&str, Option<u64>)] = &[
    ("Default", None),
    ("30 seconds", Some(30)),
    ("1 minute", Some(60)),
    ("2 minutes", Some(120)),
    ("5 minutes", Some(300)),
    ("10 minutes", Some(600)),
    ("15 minutes", Some(900)),
    ("30 minutes", Some(1800)),
    ("1 hour", Some(3600)),
];

pub(super) struct Draft {
    pub id: &'static str,
    pub enabled: bool,
    pub interval: Choice,
    initial: Preferences,
    intervals: Vec<(String, Option<u64>)>,
}

pub(super) fn load(
    boards: &[crate::registry::BoardRegistration],
    window: &mut Window,
    cx: &mut App,
) -> Vec<Draft> {
    boards
        .iter()
        .filter(|board| board.settings.background_updates)
        .map(|board| Draft::new(board.id, window, cx))
        .collect()
}

pub(super) fn save(drafts: &mut [Draft], cx: &mut App) -> Result<(), String> {
    let previous = cx.try_global::<Prefs>().cloned();
    for draft in drafts.iter() {
        draft.apply(cx);
    }
    if let Err(error) = prefs::save(cx) {
        if let Some(previous) = previous {
            cx.set_global(previous);
        }
        return Err(error.to_string());
    }
    for draft in drafts {
        draft.accept(cx);
    }
    Ok(())
}

fn interval_choices(saved: Option<u64>) -> Vec<(String, Option<u64>)> {
    let mut intervals: Vec<_> = INTERVALS
        .iter()
        .map(|(label, seconds)| ((*label).to_owned(), *seconds))
        .collect();
    if let Some(seconds) = saved
        && !intervals.iter().any(|(_, value)| *value == saved)
    {
        intervals.push((format!("{seconds} seconds (saved)"), saved));
    }
    intervals
}

impl Draft {
    fn new(id: &'static str, window: &mut Window, cx: &mut App) -> Self {
        let initial = refresh::preferences(id, cx);
        let intervals = interval_choices(initial.interval_seconds);
        let selected = intervals
            .iter()
            .find(|(_, value)| *value == initial.interval_seconds)
            .map(|(label, _)| label.as_str());
        let interval = choice(
            intervals.iter().map(|(label, _)| label.clone()).collect(),
            selected,
            false,
            window,
            cx,
        );
        Self {
            id,
            enabled: initial.enabled,
            interval,
            initial,
            intervals,
        }
    }

    /// The interval labels the select offers, in order.
    pub(super) fn labels(&self) -> impl Iterator<Item = &str> {
        self.intervals.iter().map(|(label, _)| label.as_str())
    }

    pub(super) fn preferences(&self, cx: &App) -> Preferences {
        let selected = self.interval.read(cx).selected_value();
        let interval_seconds = self
            .intervals
            .iter()
            .find(|(label, _)| Some(label) == selected)
            .map_or(self.initial.interval_seconds, |(_, value)| *value);
        Preferences {
            enabled: self.enabled,
            interval_seconds,
        }
    }

    /// Preserve concurrent changes to each field the user left untouched.
    pub(super) fn apply(&self, cx: &mut App) {
        let draft = self.preferences(cx);
        if draft == self.initial {
            return;
        }
        let current = refresh::preferences(self.id, cx);
        Preferences {
            enabled: if draft.enabled == self.initial.enabled {
                current.enabled
            } else {
                draft.enabled
            },
            interval_seconds: if draft.interval_seconds == self.initial.interval_seconds {
                current.interval_seconds
            } else {
                draft.interval_seconds
            },
        }
        .save(self.id, cx);
    }

    pub(super) fn accept(&mut self, cx: &App) {
        self.initial = self.preferences(cx);
    }
}
