//! Splitting long text so each relayed message fits in 512 bytes.

use crate::codec::BODY_LIMIT;

/// The longest hostname servers commonly relay in a source.
const HOST_MAX: usize = 63;
/// The fewest bytes a piece may hold, however long the overhead.
const MIN_PIECE: usize = 64;

/// Bytes of text one `command target :text` message can carry once the
/// server prefixes it with `:nick!user@host `. `extra` is any wrapper, such
/// as CTCP `ACTION`.
pub fn budget(command: &str, nick: &str, user: &str, target: &str, extra: usize) -> usize {
    let source = ":!@ ".len() + nick.len() + user.len() + HOST_MAX;
    let fixed = command.len() + target.len() + "  :\r\n".len();
    BODY_LIMIT
        .saturating_sub(source + fixed + extra)
        .max(MIN_PIECE)
}

/// `text` in pieces of at most `limit` bytes, split at the last space
/// before the limit when there is one, else at a character boundary.
pub fn split_text(text: &str, limit: usize) -> Vec<String> {
    split(text, limit, false)
}

/// Multiline continuations concatenate directly, so keep boundary spaces.
pub(super) fn split_multiline_text(text: &str, limit: usize) -> Vec<String> {
    split(text, limit, true)
}

fn split(text: &str, limit: usize, preserve_spaces: bool) -> Vec<String> {
    let limit = limit.max(1);
    let mut pieces = Vec::new();
    let mut rest = text;
    while rest.len() > limit {
        let mut cut = limit;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (piece, next) = match rest[..cut].rfind(' ') {
            Some(space) if space > 0 => (
                &rest[..space + usize::from(preserve_spaces)],
                &rest[space + 1..],
            ),
            _ if cut == 0 => {
                let width = rest.chars().next().map_or(1, char::len_utf8);
                (&rest[..width], &rest[width..])
            }
            _ => (&rest[..cut], &rest[cut..]),
        };
        pieces.push(piece.to_string());
        rest = next;
    }
    if !rest.is_empty() {
        pieces.push(rest.to_string());
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::{budget, split_text};

    #[test]
    fn short_text_is_one_piece() {
        assert_eq!(split_text("hello", 10), vec!["hello"]);
        assert!(split_text("", 10).is_empty());
    }

    #[test]
    fn splits_at_spaces() {
        assert_eq!(
            split_text("the quick brown fox", 10),
            vec!["the quick", "brown fox"]
        );
    }

    #[test]
    fn splits_long_words_at_char_boundaries() {
        let pieces = split_text("ééééé", 5);
        assert_eq!(pieces, vec!["éé", "éé", "é"]);
        assert!(pieces.iter().all(|piece| piece.len() <= 5));
        assert_eq!(split_text("abcdef", 4), vec!["abcd", "ef"]);
    }

    #[test]
    fn budget_leaves_room_for_the_relayed_prefix() {
        let room = budget("PRIVMSG", "nick", "user", "#chan", 0);
        let relayed = ":nick!user@".len() + 63 + " PRIVMSG #chan :".len() + room + 2;
        assert_eq!(relayed, 512);
        assert_eq!(budget("PRIVMSG", "n", "u", &"#".repeat(600), 0), 64);
    }
}
