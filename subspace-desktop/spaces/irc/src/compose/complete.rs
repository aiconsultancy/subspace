//! Classic Tab completion of the nick being typed before the cursor, for
//! a word without `@` while no completion list is open.

use std::ops::Range;

use gpui_kit::{Context, Window};
use subspace_framework::dashboard::chat_input::completion::word_before;

use super::suggest::{Facts, fold};
use crate::conversation::Conversation;

/// The word before `cursor` and the first matching nick, in order of
/// `nicks`, to replace it with. A nick that starts the line gets the `: `
/// address suffix; elsewhere a space follows.
pub(crate) fn complete(
    text: &str,
    cursor: usize,
    nicks: &[&str],
) -> Option<(Range<usize>, String)> {
    let range = word_before(text, cursor);
    let word = &text[range.clone()];
    if word.is_empty() {
        return None;
    }
    let folded = fold(word);
    let nick = nicks
        .iter()
        .find(|nick| fold(nick).starts_with(&folded) && nick.len() > word.len())?;
    let suffix = if range.start == 0 { ": " } else { " " };
    Some((range, format!("{nick}{suffix}")))
}

pub(super) fn apply(
    conversation: &mut Conversation,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let facts = Facts::gather(&conversation.hub.read(cx).state, &conversation.buffer);
    let names: Vec<&str> = facts
        .people
        .iter()
        .map(|person| person.nick.as_str())
        .collect();
    let input = conversation.draft.entity.clone();
    let (text, cursor) = input.read_with(cx, |input, cx| (input.text(cx), input.cursor(cx)));
    if let Some((range, nick)) = complete(&text, cursor, &names) {
        input.update(cx, |input, cx| {
            input.replace_range(range, &nick, window, cx)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::complete;

    fn completed(text: &str, nicks: &[&str]) -> Option<String> {
        complete(text, text.len(), nicks).map(|(range, nick)| {
            let mut text = text.to_owned();
            text.replace_range(range, &nick);
            text
        })
    }

    #[test]
    fn the_word_before_the_cursor_completes_to_a_member_with_an_address_at_line_start() {
        let nicks = ["alice", "Albert", "bob"];
        assert_eq!(completed("al", &nicks).as_deref(), Some("alice: "));
        assert_eq!(completed("hi BO", &nicks).as_deref(), Some("hi bob "));
        assert_eq!(completed("hi ", &nicks), None);
        assert_eq!(completed("hi zed", &nicks), None);
        assert_eq!(completed("bob", &nicks), None, "already complete");
        assert_eq!(
            complete("al and more", 2, &nicks),
            Some((0..2, "alice: ".to_owned())),
            "in the middle of a line"
        );
    }
}
