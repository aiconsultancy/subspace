//! Up and Down through the lines sent from one composer.

use gpui_kit::{App, Context, Window};

use crate::conversation::Conversation;

/// Sent lines kept per composer.
const KEPT: usize = 100;

#[derive(Debug, Default)]
pub(crate) struct Recall {
    sent: Vec<String>,
    /// The recalled line, counted from the oldest.
    at: Option<usize>,
    /// What was typed before recalling started, restored past the newest.
    draft: String,
}

impl Recall {
    pub fn push(&mut self, text: String) {
        if self.sent.last() != Some(&text) {
            self.sent.push(text);
        }
        if self.sent.len() > KEPT {
            self.sent.remove(0);
        }
        self.at = None;
        self.draft.clear();
    }

    /// The previous sent line, keeping `current` to come back to.
    pub fn older(&mut self, current: &str) -> Option<String> {
        let at = match self.at {
            None => {
                self.draft = current.to_owned();
                self.sent.len().checked_sub(1)?
            }
            Some(0) => return None,
            Some(at) => at - 1,
        };
        self.at = Some(at);
        self.sent.get(at).cloned()
    }

    /// Whether `text` is the recalled line, unedited.
    pub fn showing(&self, text: &str) -> bool {
        self.at
            .and_then(|at| self.sent.get(at))
            .is_some_and(|line| line == text)
    }

    /// The next sent line, then the kept draft.
    pub fn newer(&mut self) -> Option<String> {
        let at = self.at? + 1;
        if at < self.sent.len() {
            self.at = Some(at);
            return self.sent.get(at).cloned();
        }
        self.at = None;
        Some(std::mem::take(&mut self.draft))
    }
}

/// Keeps Up and Down recalling while the draft `text` is the recalled line,
/// also when that line has several lines.
pub(super) fn follow(conversation: &Conversation, text: &str, cx: &mut App) {
    let recalled = conversation.draft.recall.showing(text);
    conversation
        .draft
        .entity
        .update(cx, |input, cx| input.set_recalled(recalled, cx));
}

pub(super) fn step(
    conversation: &mut Conversation,
    older: bool,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let batches = super::batches_lines(conversation, cx);
    let draft = &mut conversation.draft;
    let current = draft.entity.read(cx).text(cx);
    let next = if older {
        draft.recall.older(&current)
    } else {
        draft.recall.newer()
    };
    if let Some(text) = next {
        let recalled = draft.recall.showing(&text);
        draft.entity.update(cx, |input, cx| {
            input.set_text(&text, window, cx);
            input.set_recalled(recalled, cx);
        });
        super::validate_draft(draft, &text, batches, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::Recall;

    #[test]
    fn up_walks_back_and_down_returns_to_the_unsent_draft() {
        let mut recall = Recall::default();
        assert_eq!(recall.older("typing"), None);
        recall.push("first".into());
        recall.push("second".into());
        recall.push("second".into());
        assert_eq!(recall.older("draft").as_deref(), Some("second"));
        assert_eq!(recall.older("second").as_deref(), Some("first"));
        assert_eq!(recall.older("first"), None, "stops at the oldest");
        assert_eq!(recall.newer().as_deref(), Some("second"));
        assert_eq!(recall.newer().as_deref(), Some("draft"));
        assert_eq!(recall.newer(), None);
    }

    #[test]
    fn only_the_unedited_recalled_line_is_showing() {
        let mut recall = Recall::default();
        recall.push("one\ntwo".into());
        assert!(!recall.showing("one\ntwo"), "nothing recalled yet");
        assert_eq!(recall.older("").as_deref(), Some("one\ntwo"));
        assert!(recall.showing("one\ntwo"));
        assert!(!recall.showing("one\ntwo!"), "edited");
        recall.newer();
        assert!(!recall.showing("one\ntwo"), "back at the draft");
    }
}
