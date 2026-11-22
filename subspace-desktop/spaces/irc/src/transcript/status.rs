//! The status line under a request, from [`crate::thread_status`]: while it
//! runs, who works on it and what they are doing, a clock counting from the
//! request and a small stage meter; once it ends, a quiet settled line such
//! as `Done in 42 m`. GPUI Kit's `Marker`, the row Kit offers for
//! conversation status, with Kit's `Progress` as the meter and the shared
//! warning pill while the last update may be out of date.

use chrono::{DateTime, Local, TimeDelta, Utc};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::marker::{Marker, MarkerContent, MarkerIcon};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::badge::{Pill, PillTone};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};

use crate::state::Entry;
use crate::thread_status::{Outcome, Settled, Shown, Stage, Statuses};

/// Seconds in a minute and an hour, for the clock and durations.
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;

/// The status line under `entry`, aligned with the message body, when it is
/// a request with progress or an ending. Read-only: no focus or action.
/// `now` sets a running line's clock.
pub(super) fn line(
    entry: &Entry,
    statuses: &Statuses,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    let shown = statuses.get(&entry.seq)?;
    let request = entry.line.id.as_deref()?;
    let id = format!("irc-status:{request}");
    Some(match shown.settled {
        Some(settled) => settled_line(&id, shown, settled, tokens),
        None => status_line(&id, shown, now, tokens, cx),
    })
}

/// One running request's line, such as `astra · Using a tool 0:35`, then
/// the stage meter and its word, updated in place.
fn status_line(
    id: &str,
    status: &Shown,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    let since = status
        .stale
        .map(|time| time.with_timezone(&Local).format("%H:%M").to_string());
    let text = format!("{} · {}", status.agent, status.label);
    let clock = since.is_none().then(|| clock(now - status.since));
    let marker = Marker::new()
        .text_size(tokens.caption_size())
        .icon(MarkerIcon::new().child(Icon::from(glyph(since.is_some())).xsmall()))
        .content(MarkerContent::new().text(text))
        .children(clock.map(|clock| clock_text(id, clock, tokens, cx)))
        .child(meter(id, status.stage, tokens))
        .children(since.as_deref().map(|at| stale(id, at)));
    frame(id, spoken(status, since.as_deref()), marker)
}

/// A finished request's quiet line: `Done in 42 m`, `Failed after 3 m`, or
/// `Done` for an ending stored before its time was kept.
fn settled_line(id: &str, status: &Shown, settled: Settled, tokens: &Tokens) -> AnyElement {
    let text = settled_text(settled);
    let (icon, tone) = settled_glyph(settled);
    let spoken = settled_spoken(&status.agent, &text);
    let marker = Marker::new()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .icon(MarkerIcon::new().child(Icon::from(icon).xsmall().text_color(tokens.tone(tone))))
        .content(MarkerContent::new().text(text));
    frame(id, spoken, marker)
}

/// A quiet check once done, a cross in the red tone once failed, and a
/// quiet plain circle when how it ended is unknown.
fn settled_glyph(settled: Settled) -> (IconName, Tone) {
    match settled.outcome {
        Some(Outcome::Failed) => (IconName::CircleX, Tone::Red),
        Some(Outcome::Completed) => (IconName::CircleCheck, Tone::Muted),
        None => (IconName::Circle, Tone::Muted),
    }
}

/// A settled line's accessible name: the agent, then the text.
fn settled_spoken(agent: &str, text: &str) -> String {
    if agent.is_empty() {
        text.to_owned()
    } else {
        format!("{agent}: {text}")
    }
}

/// The line's frame around `marker`: a status region named `spoken`.
fn frame(id: &str, spoken: String, marker: Marker) -> AnyElement {
    div()
        .id(SharedString::from(id.to_owned()))
        .role(Role::Status)
        .aria_label(spoken)
        .test_support()
        .py_1()
        .child(marker)
        .into_any_element()
}

/// `Done in 42 m`, `Failed after 3 m`, or without a duration when the
/// ending's time is unknown; `Ended` when how it ended is unknown, as for an
/// ending stored before outcomes were kept, which never claims success.
pub(crate) fn settled_text(settled: Settled) -> String {
    let (word, joint) = match settled.outcome {
        Some(Outcome::Completed) => ("Done", "in"),
        Some(Outcome::Failed) => ("Failed", "after"),
        None => ("Ended", "after"),
    };
    match settled.took.map(duration) {
        Some(took) => format!("{word} {joint} {took}"),
        None => word.to_owned(),
    }
}

/// A finished request's duration, coarse as the line is quiet: `35 s`,
/// `42 m`, or `2 h 5 m`.
pub(crate) fn duration(took: TimeDelta) -> String {
    let seconds = took.num_seconds().max(0);
    match seconds {
        0..MINUTE => format!("{seconds} s"),
        MINUTE..HOUR => format!("{} m", seconds / MINUTE),
        _ => format!("{} h {} m", seconds / HOUR, seconds % HOUR / MINUTE),
    }
}

/// A running request's clock since it was sent: `0:35`, `12:04`, or
/// `1:02:03` past an hour.
pub(crate) fn clock(elapsed: TimeDelta) -> String {
    let seconds = elapsed.num_seconds().max(0);
    let (hours, minutes, rest) = (seconds / HOUR, seconds % HOUR / MINUTE, seconds % MINUTE);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

/// The clock in quiet monospace, so it does not jump as it ticks.
fn clock_text(id: &str, clock: String, tokens: &Tokens, cx: &App) -> AnyElement {
    div()
        .id(SharedString::from(format!("{id}:clock")))
        .test_support()
        .font_family(cx.theme().mono_font_family.clone())
        .text_color(tokens.muted)
        .child(clock)
        .into_any_element()
}

/// Kit's progress bar as the stage meter, one fifth per stage, then the
/// stage's word.
fn meter(id: &str, stage: Stage, tokens: &Tokens) -> AnyElement {
    h_flex()
        .gap_1()
        .child(
            div().w_16().child(
                Progress::new(SharedString::from(format!("{id}:meter")))
                    .xsmall()
                    .value(stage.percent())
                    .accessibility_label(meter_label(stage)),
            ),
        )
        .child(div().text_color(tokens.muted).child(stage.word()))
        .into_any_element()
}

/// The meter's accessible name, such as *Stage 2 of 5: working*.
fn meter_label(stage: Stage) -> String {
    format!(
        "Stage {} of {}: {}",
        stage.step(),
        Stage::ALL.len(),
        stage.word()
    )
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

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::{clock, duration, settled_glyph, settled_text};
    use crate::thread_status::{Outcome, Settled};
    use subspace_framework::dashboard::panel::IconName;

    #[test]
    fn the_clock_counts_minutes_and_seconds_then_hours() {
        assert_eq!(clock(TimeDelta::seconds(35)), "0:35");
        assert_eq!(clock(TimeDelta::seconds(12 * 60 + 4)), "12:04");
        assert_eq!(clock(TimeDelta::seconds(3723)), "1:02:03");
        assert_eq!(
            clock(TimeDelta::seconds(-5)),
            "0:00",
            "never runs backwards"
        );
    }

    #[test]
    fn a_settled_line_says_how_it_ended_and_how_long_it_took() {
        let settled = |outcome, seconds: Option<i64>| Settled {
            outcome,
            took: seconds.map(TimeDelta::seconds),
        };
        let (done, failed) = (Some(Outcome::Completed), Some(Outcome::Failed));
        assert_eq!(settled_text(settled(done, Some(42 * 60))), "Done in 42 m");
        assert_eq!(settled_text(settled(failed, Some(35))), "Failed after 35 s");
        assert_eq!(settled_text(settled(done, None)), "Done");
        assert_eq!(settled_text(settled(failed, None)), "Failed");
        assert_eq!(settled_text(settled(None, None)), "Ended", "never Done");
        assert_eq!(settled_glyph(settled(None, None)).0, IconName::Circle);
        assert_eq!(duration(TimeDelta::seconds(2 * 3600 + 5 * 60)), "2 h 5 m");
    }
}
