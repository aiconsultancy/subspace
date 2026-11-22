//! Assembles received multiline components into one IRC message.

use crate::proto::Message;

const CONCAT: &str = "draft/multiline-concat";

pub(super) fn combine(opening: &Message, messages: Vec<Message>) -> Option<Message> {
    let mut messages = messages.into_iter();
    let mut first = messages.next()?;
    let target = first.param(0).to_owned();
    let mut text = first.trailing().to_owned();
    for message in messages {
        if message.tag(CONCAT).is_none() {
            text.push('\n');
        }
        text.push_str(message.trailing());
    }
    first.params = vec![target, text];
    // Client-only tags, the reply reference among them, come from the
    // opening `BATCH` alone, as the multiline specification requires.
    first.tags.retain(|key, _| !key.starts_with('+'));
    first.tags.extend(opening.tags.clone());
    first.tags.remove("batch");
    first.tags.remove(CONCAT);
    Some(first)
}
