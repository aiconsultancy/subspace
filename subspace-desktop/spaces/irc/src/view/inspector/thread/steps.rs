//! A request's status timeline in the Thread tab: a plain list, one row
//! per step with its words, when it began and how long it took. Only the
//! latest [`crate::thread::MAX_STEPS`] are drawn, after a line that counts
//! the earlier ones.

use chrono::Local;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::{BufferKey, Command};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone, feedback, workspace};

use crate::Irc;
use crate::thread::{Step, StepKind, Thread, Timeline};
use crate::thread_status::Outcome;
use crate::transcript::status;

/// What the timeline says when only the ending of a request is held.
pub(crate) const EARLIER_MISSING: &str = "The steps before the end are not loaded.";
/// Lines asked for around the root when its earlier steps are missing.
const AROUND: u16 = 50;

/// The request's steps, one row each, with a note when the steps before
/// its ending are not loaded.
pub(super) fn section(
    timeline: &Timeline,
    (board, buffer, thread): (&Irc, &BufferKey, &Thread),
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let last = timeline.steps.len().saturating_sub(1);
    workspace::section("Status", cx)
        .children((timeline.earlier_steps > 0).then(|| {
            feedback::empty_line(
                &tokens,
                "irc-thread-earlier-steps",
                earlier_steps_words(timeline.earlier_steps),
            )
            .id("irc-thread-earlier-steps-line")
            .test_support()
        }))
        .child(
            v_flex()
                .id("irc-thread-steps")
                .role(Role::List)
                .aria_label(format!("{} status", timeline.agent))
                .test_support()
                .w_full()
                .gap_1()
                .children(
                    timeline
                        .steps
                        .iter()
                        .enumerate()
                        .map(|(index, step)| step_row(index, step, index == last, &tokens)),
                ),
        )
        .children(
            timeline
                .earlier_missing
                .then(|| missing(board, buffer, thread, cx)),
        )
        .into_any_element()
}

/// What the timeline says of the earlier steps it leaves out.
pub(crate) fn earlier_steps_words(skipped: usize) -> String {
    match skipped {
        1 => "1 earlier step is not shown.".to_owned(),
        n => format!("{n} earlier steps are not shown."),
    }
}

/// What a step's columns say: its words, when it began, and how long it
/// took. The last step of a running request says since when instead.
pub(crate) fn step_words(step: &Step, last: bool) -> (&'static str, String, String) {
    let time = step
        .at
        .map(|at| at.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let running = last && step.took.is_none() && matches!(step.kind, StepKind::Stage(_));
    let time = if running && !time.is_empty() {
        format!("since {time}")
    } else {
        time
    };
    let took = match (step.kind, step.took) {
        (StepKind::Ended(Some(Outcome::Completed)), Some(took)) => {
            format!("in {}", status::duration(took))
        }
        (StepKind::Ended(_), Some(took)) => format!("after {}", status::duration(took)),
        (_, Some(took)) => status::duration(took),
        (_, None) => String::new(),
    };
    (step.kind.label(), time, took)
}

/// One step: its words, then when and how long in quiet text. A failed
/// ending's words take the red tone.
fn step_row(index: usize, step: &Step, last: bool, tokens: &Tokens) -> AnyElement {
    let (label, time, took) = step_words(step, last);
    let spoken = [label, &time, &took]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let failed = step.kind == StepKind::Ended(Some(Outcome::Failed));
    let tone = if failed {
        tokens.tone(Tone::Red)
    } else {
        tokens.fg
    };
    h_flex()
        .id(SharedString::from(format!("irc-thread-step:{index}")))
        .role(Role::ListItem)
        .aria_label(spoken)
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .child(div().flex_1().min_w_0().text_color(tone).child(label))
        .child(div().flex_shrink_0().text_color(tokens.muted).child(time))
        .child(div().flex_shrink_0().text_color(tokens.muted).child(took))
        .into_any_element()
}

/// The note that the earlier steps are not loaded, and, where the server
/// keeps history, the Space's way of loading the lines around a message.
fn missing(board: &Irc, buffer: &BufferKey, thread: &Thread, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let load = around(board, buffer, thread, cx);
    v_flex()
        .w_full()
        .gap_2()
        .child(
            feedback::empty_line(&tokens, "irc-thread-missing", EARLIER_MISSING)
                .id("irc-thread-missing-line")
                .test_support(),
        )
        .children(load)
        .into_any_element()
}

/// **Load around this message**, while the network is connected and keeps
/// history: the lines around the root, as a search result loads them.
fn around(board: &Irc, buffer: &BufferKey, thread: &Thread, cx: &App) -> Option<AnyElement> {
    let hub = board.hub.as_ref()?;
    let network = hub.read(cx).state.network(&buffer.network)?;
    let keeps = network.caps.iter().any(|cap| cap == "draft/chathistory");
    if !keeps || crate::thread::reply_refusal(Some(network)) == Some(crate::thread::NOT_CONNECTED) {
        return None;
    }
    let command = Command::LoadAround {
        buffer: buffer.clone(),
        id: thread.root.line.id.clone(),
        time: thread.root.line.time,
        limit: AROUND,
    };
    let hub = hub.downgrade();
    Some(
        action_button(
            "irc-thread-load-around",
            Intent::Quiet,
            Some(IconName::Clock),
            "Load around this message",
        )
        .on_click(move |_, _, cx| {
            _ = hub.update(cx, |hub, _| hub.send(command.clone()));
        })
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeDelta};

    use super::{earlier_steps_words, step_words};
    use crate::fixtures::at;
    use crate::thread::{Step, StepKind};
    use crate::thread_status::{Outcome, Stage};

    fn step(kind: StepKind, minutes: Option<i64>) -> Step {
        Step {
            kind,
            at: Some(at(2)),
            took: minutes.map(TimeDelta::minutes),
        }
    }

    #[test]
    fn a_step_says_when_it_began_and_how_long_it_took() {
        let time = at(2).with_timezone(&Local).format("%H:%M").to_string();
        let working = StepKind::Stage(Stage::Working);
        assert_eq!(
            step_words(&step(working, Some(6)), false),
            ("Working", time.clone(), "6 m".to_owned())
        );
        assert_eq!(
            step_words(&step(working, None), true),
            ("Working", format!("since {time}"), String::new()),
            "the running step says since when"
        );
        let done = StepKind::Ended(Some(Outcome::Completed));
        assert_eq!(step_words(&step(done, Some(42)), true).2, "in 42 m");
        let failed = StepKind::Ended(Some(Outcome::Failed));
        assert_eq!(step_words(&step(failed, Some(3)), true).2, "after 3 m");
        let unknown = Step {
            kind: StepKind::Ended(None),
            at: None,
            took: None,
        };
        assert_eq!(
            step_words(&unknown, true),
            ("Ended", String::new(), String::new())
        );
    }

    #[test]
    fn the_left_out_steps_are_counted() {
        assert_eq!(earlier_steps_words(1), "1 earlier step is not shown.");
        assert_eq!(earlier_steps_words(97), "97 earlier steps are not shown.");
    }
}
