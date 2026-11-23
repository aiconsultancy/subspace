//! The **Thread** tab: the chosen thread of the conversation in front, top
//! to bottom its root message, a request's status timeline as a plain list,
//! then the replies grouped by author as the transcript groups them, or an
//! honest line when there are none. With no thread chosen it says how to
//! open one. Its footer holds **Reply in thread**, or why a reply can't be
//! sent here. See [`crate::thread`].

mod footer;
mod steps;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};
use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::{Tokens, feedback, workspace};

pub(super) use footer::footer;

use crate::Irc;
use crate::thread::Thread;
use crate::transcript::{RowContext, thread_message};

/// What the tab says before a thread is chosen.
pub(crate) const PROMPT: &str =
    "Choose a message's replies or a request's status line to see its thread here.";
/// What the tab says for a thread with no replies yet.
pub(crate) const NO_REPLIES: &str = "No replies yet.";

/// The tab's body for the conversation in front.
pub(super) fn body(board: &Irc, cx: &App) -> AnyElement {
    let shown = board
        .dock
        .focused
        .as_ref()
        .and_then(|buffer| Some((buffer, board.threads.shown(buffer)?)));
    let context = shown
        .as_ref()
        .and_then(|(buffer, _)| row_context(board, buffer, cx));
    match (shown, context) {
        (Some((buffer, thread)), Some(context)) => content(&thread, (board, buffer), &context, cx),
        _ => prompt(cx),
    }
}

/// The conversation's row facts, with no keyboard focus or search result
/// marked, so the tab draws its messages as the transcript does.
fn row_context(board: &Irc, buffer: &BufferKey, cx: &App) -> Option<RowContext> {
    let hosted = board.dock.open.get(buffer)?;
    let mut context = hosted.entity.read(cx).row_context(cx);
    context.focus = None;
    context.card = None;
    context.target = None;
    Some(context)
}

/// The one line that says how to open a thread.
fn prompt(cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    feedback::empty_line(&tokens, "irc-thread-prompt", PROMPT)
        .id("irc-thread-prompt-line")
        .role(Role::Note)
        .aria_label(PROMPT)
        .test_support()
        .into_any_element()
}

/// The root, the timeline and the replies.
fn content(
    thread: &Thread,
    (board, buffer): (&Irc, &BufferKey),
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    v_flex()
        .id("irc-thread")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_6()
        .child(thread_message(&thread.root, false, context, cx))
        .children(
            thread
                .timeline
                .as_ref()
                .map(|timeline| steps::section(timeline, (board, buffer, thread), cx)),
        )
        .child(replies(thread, context, cx))
        .into_any_element()
}

/// The replies under their heading and count, grouped by author, or the
/// line that says there are none yet.
/// The most replies the tab draws: the latest, with the rest counted.
pub(crate) const MAX_REPLIES: usize = 40;

/// How many of `count` replies, the earliest, the tab leaves out.
pub(crate) fn earlier(count: usize) -> usize {
    count.saturating_sub(MAX_REPLIES)
}

/// What the tab says of the earlier replies it leaves out.
pub(crate) fn earlier_words(skipped: usize) -> String {
    match skipped {
        1 => "1 earlier reply is in the channel.".to_owned(),
        n => format!("{n} earlier replies are in the channel."),
    }
}

fn replies(thread: &Thread, context: &RowContext, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let title = match thread.replies.len() {
        0 => "Replies".to_owned(),
        count => format!("Replies · {count}"),
    };
    let section = workspace::section(title, cx);
    if thread.replies.is_empty() {
        return section
            .child(
                feedback::empty_line(&tokens, "irc-thread-no-replies", NO_REPLIES)
                    .id("irc-thread-no-replies-line")
                    .test_support(),
            )
            .into_any_element();
    }
    // The latest replies only, so a long thread draws a bounded list; the
    // heading still counts every loaded reply.
    let skipped = earlier(thread.replies.len());
    let shown = &thread.replies[skipped..];
    let continues = crate::thread::continues(shown);
    section
        .children((skipped > 0).then(|| {
            feedback::empty_line(&tokens, "irc-thread-earlier", earlier_words(skipped))
                .id("irc-thread-earlier-line")
                .test_support()
        }))
        .child(
            v_flex()
                .id("irc-thread-replies")
                .test_support()
                .w_full()
                .min_w_0()
                .gap_1()
                .children(
                    shown
                        .iter()
                        .zip(continues)
                        .map(|(entry, continues)| thread_message(entry, continues, context, cx)),
                ),
        )
        .into_any_element()
}
