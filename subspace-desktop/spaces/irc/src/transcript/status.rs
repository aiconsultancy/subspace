//! The status line under a request, from [`crate::thread_status`]: while it
//! runs, who works on it and what they are doing, a clock counting from the
//! request and an activity mark; once it ends, a quiet line such as
//! `Done in 42 m`. The warning pill identifies an out-of-date status.

use chrono::{DateTime, Local, TimeDelta, Utc};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::marker::{Marker, MarkerContent, MarkerIcon};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::activity_mark::ActivityMark;
use subspace_framework::dashboard::badge::{Pill, PillTone};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};

use std::rc::Rc;

use super::replies::{ThreadLink, replies_text};
use crate::state::Entry;
use crate::thread_status::{Outcome, Settled, Shown, Stage, Statuses};

/// Seconds in a minute and an hour, for the clock and durations.
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const ACTIVITY_SIZE: f32 = 14.;

/// The status line under `entry`, aligned with the message body, when it is
/// a request with progress or an ending. `now` sets a running line's clock.
/// With a thread `link` the line opens the request's thread and ends with
/// its reply count, when it has replies; without one it is read-only.
pub(super) fn line(
    entry: &Entry,
    (statuses, link): (&Statuses, Option<&ThreadLink>),
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    let shown = statuses.get(&entry.seq)?;
    let request = entry.line.id.as_deref()?;
    let id = format!("irc-status:{request}");
    let (spoken, marker) = match shown.settled {
        Some(settled) => settled_line(shown, settled, tokens),
        None => status_line(&id, shown, now, tokens, cx),
    };
    Some(frame(&id, spoken, marker, (link, tokens)))
}

/// One running request's line, such as `astra · Using a tool 0:35`, updated in place.
fn status_line(
    id: &str,
    status: &Shown,
    now: DateTime<Utc>,
    tokens: &Tokens,
    cx: &App,
) -> (String, Marker) {
    let since = status
        .stale
        .map(|time| time.with_timezone(&Local).format("%H:%M").to_string());
    let text = format!("{} · {}", status.agent, status.label);
    let clock = since.is_none().then(|| clock(now - status.since));
    let marker = Marker::new()
        .text_size(tokens.caption_size())
        .icon(MarkerIcon::new().child(status_icon(id, status)))
        .content(MarkerContent::new().text(text))
        .children(clock.map(|clock| clock_text(id, clock, tokens, cx)))
        .children(since.as_deref().map(|at| stale(id, at)));
    (spoken(status, since.as_deref()), marker)
}

/// A finished request's quiet line: `Done in 42 m`, `Failed after 3 m`, or
/// `Done` for an ending stored before its time was kept.
fn settled_line(status: &Shown, settled: Settled, tokens: &Tokens) -> (String, Marker) {
    let text = settled_text(settled);
    let (icon, tone) = settled_glyph(settled);
    let spoken = settled_spoken(&status.agent, &text);
    let marker = Marker::new()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .icon(MarkerIcon::new().child(Icon::from(icon).xsmall().text_color(tokens.tone(tone))))
        .content(MarkerContent::new().text(text));
    (spoken, marker)
}

/// A quiet check once done, a cross in the red tone once failed, and a
/// quiet plain circle when how it ended is unknown.
pub(crate) fn settled_glyph(settled: Settled) -> (IconName, Tone) {
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

/// The line's frame around `marker`: a status region named `spoken`, or,
/// with a thread `link`, a button that opens the thread, ending with its
/// reply count when it has replies.
fn frame(
    id: &str,
    spoken: String,
    marker: Marker,
    (link, tokens): (Option<&ThreadLink>, &Tokens),
) -> AnyElement {
    let frame = div()
        .id(SharedString::from(id.to_owned()))
        .test_support()
        .py_1();
    let Some(link) = link else {
        return frame
            .role(Role::Status)
            .aria_label(spoken)
            .child(marker)
            .into_any_element();
    };
    let count = (link.count > 0).then(|| replies_text(link.count));
    let open = Rc::clone(&link.open);
    let button = frame
        .role(Role::Button)
        .aria_label(linked_spoken(&spoken, count.as_deref()))
        .cursor_pointer()
        .on_click(move |_, window, cx| open(window, cx))
        .child(marker.children(count.map(|count| {
            div()
                .id(SharedString::from(format!("{id}:replies")))
                .test_support()
                .text_color(tokens.muted)
                .child(format!("· {count} ›"))
        })));
    // The line is also a status beside the button, not inside it, so its
    // stage and clock are announced however nested roles are flattened.
    // It takes no room: the button draws the line.
    let live = div()
        .id(SharedString::from(format!("{id}:live")))
        .test_support()
        .role(Role::Status)
        .aria_label(spoken)
        .size_0()
        .overflow_hidden();
    div().child(button).child(live).into_any_element()
}

/// A linked line's accessible name: the line, its reply count when it has
/// replies, and what choosing it does.
fn linked_spoken(spoken: &str, count: Option<&str>) -> String {
    match count {
        Some(count) => format!("{spoken} · {count}. Opens the thread"),
        None => format!("{spoken}. Opens the thread"),
    }
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

/// What assistive technology reads, with when the line may be out of date.
fn spoken(status: &Shown, since: Option<&str>) -> String {
    let line = format!("{}: {}", status.agent, status.label);
    match since {
        Some(at) => format!("{line}. Stale: the last update was at {at}"),
        None => line,
    }
}

/// Decorative: the status text provides the accessible state.
fn status_icon(id: &str, status: &Shown) -> AnyElement {
    let glyph = match (status.stale, status.active, status.stage) {
        (Some(_), _, _) => IconName::Unplug,
        (None, true, _) => {
            let id = SharedString::from(format!("{id}:activity"));
            return div()
                .id(id.clone())
                .test_support()
                .child(ActivityMark::new(id, px(ACTIVITY_SIZE)))
                .into_any_element();
        }
        (None, false, Stage::Waiting | Stage::Working) => IconName::Pause,
        _ => IconName::Bot,
    };
    Icon::from(glyph).xsmall().into_any_element()
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
