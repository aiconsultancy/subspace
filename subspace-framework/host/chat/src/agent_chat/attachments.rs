//! Splits a replayed user prompt into the message the user typed and the
//! references the runtime appended, so the transcript can fold them away.
//! Pure, so the chat view keeps no parsing of its own.

use subspace_agent::ATTACHMENT_MARKER;

/// One attached reference: its first line is the label, the rest the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Attachment {
    pub label: String,
    pub content: String,
}

/// The typed message and every reference appended after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UserMessage {
    pub message: String,
    pub attachments: Vec<Attachment>,
}

fn attachment(block: &str) -> Attachment {
    match block.split_once('\n') {
        Some((label, content)) => Attachment {
            label: label.to_owned(),
            content: content.to_owned(),
        },
        None => Attachment {
            label: block.to_owned(),
            content: String::new(),
        },
    }
}

/// Splits `text` on the runtime's attachment marker. Text without the marker
/// is the message alone.
pub(super) fn split_user_message(text: &str) -> UserMessage {
    let mut blocks = text.split(ATTACHMENT_MARKER);
    let message = blocks.next().unwrap_or_default().to_owned();
    UserMessage {
        message,
        attachments: blocks.map(attachment).collect(),
    }
}

/// The expansion key of one reference, distinct from the entry's own key.
pub(super) fn attachment_key(entry: &str, index: usize) -> String {
    format!("{entry}#ref{index}")
}

/// The one-line row text: the reference label and its fold marker.
pub(super) fn attachment_label(label: &str, expanded: bool) -> String {
    let marker = if expanded { "▾" } else { "▸" };
    format!("{label} {marker}")
}

#[cfg(test)]
mod tests {
    use super::{attachment_key, attachment_label, split_user_message};
    use subspace_agent::ATTACHMENT_MARKER;

    #[test]
    fn plain_text_is_the_message_alone() {
        let split = split_user_message("Review this");
        assert_eq!(split.message, "Review this");
        assert!(split.attachments.is_empty());
    }

    #[test]
    fn every_reference_splits_into_label_and_content() {
        let text = format!(
            "Review this{ATTACHMENT_MARKER}GH: octo/app#12\nService: GitHub\nState: Open\
             {ATTACHMENT_MARKER}GitHub: #101"
        );
        let split = split_user_message(&text);
        assert_eq!(split.message, "Review this");
        assert_eq!(split.attachments.len(), 2);
        assert_eq!(split.attachments[0].label, "GH: octo/app#12");
        assert_eq!(split.attachments[0].content, "Service: GitHub\nState: Open");
        assert_eq!(split.attachments[1].label, "GitHub: #101");
        assert_eq!(split.attachments[1].content, "");
    }

    #[test]
    fn reference_keys_differ_per_entry_and_index() {
        assert_eq!(attachment_key("e1", 0), "e1#ref0");
        assert_ne!(attachment_key("e1", 0), attachment_key("e1", 1));
        assert_ne!(attachment_key("e1", 0), attachment_key("e2", 0));
    }

    #[test]
    fn the_row_marker_follows_the_fold_state() {
        assert_eq!(
            attachment_label("GH: octo/app#12", false),
            "GH: octo/app#12 ▸"
        );
        assert_eq!(
            attachment_label("GH: octo/app#12", true),
            "GH: octo/app#12 ▾"
        );
    }
}
