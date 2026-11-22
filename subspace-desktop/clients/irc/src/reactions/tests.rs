use super::agent_stop::{AGENT_STOP_REACTIONS, is_agent_stop_reaction, reaction_key};
use super::{check_reaction, reactions_supported, validate_msgid, validate_reaction};
use crate::model::ReactionError;

fn caps(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn support_follows_message_tags() {
    assert!(reactions_supported(&caps(&["batch", "message-tags"])));
    assert!(!reactions_supported(&caps(&["batch", "echo-message"])));
    assert!(!reactions_supported(&[]));
}

#[test]
fn reactions_are_short_text_without_controls_or_padding() {
    for valid in [
        "👍",
        "lol",
        ":)",
        "🇬🇧",
        "👨\u{200D}👩\u{200D}👧",
        "thumbs up",
        &"x".repeat(32),
    ] {
        assert_eq!(validate_reaction(valid), Ok(valid));
    }
    for invalid in [
        "",
        " ",
        " 👍",
        "👍 ",
        "a\nb",
        "a\u{0}",
        "a\u{7F}",
        &"x".repeat(33),
    ] {
        assert_eq!(
            validate_reaction(invalid),
            Err(ReactionError::InvalidReaction)
        );
    }
}

#[test]
fn message_ids_must_be_present_short_and_unspaced() {
    assert_eq!(validate_msgid(Some("01HXYZ")), Ok("01HXYZ"));
    assert_eq!(validate_msgid(None), Err(ReactionError::NoMessageId));
    assert_eq!(validate_msgid(Some("")), Err(ReactionError::NoMessageId));
    for invalid in ["a b", "a\u{0}", &"m".repeat(129)] {
        assert_eq!(
            validate_msgid(Some(invalid)),
            Err(ReactionError::InvalidMessageId)
        );
    }
}

#[test]
fn the_check_reports_the_first_problem() {
    let tags = caps(&["message-tags"]);
    assert_eq!(check_reaction(&tags, true, Some("m1"), "👍"), Ok(()));
    assert_eq!(
        check_reaction(&[], false, None, ""),
        Err(ReactionError::Unsupported)
    );
    assert_eq!(
        check_reaction(&tags, false, None, ""),
        Err(ReactionError::NoConversation)
    );
    assert_eq!(
        check_reaction(&tags, true, None, ""),
        Err(ReactionError::NoMessageId)
    );
    assert_eq!(
        check_reaction(&tags, true, Some("m1"), ""),
        Err(ReactionError::InvalidReaction)
    );
}

#[test]
fn stargate_stop_reactions_ignore_skin_tones_and_the_variation_selector() {
    assert_eq!(AGENT_STOP_REACTIONS, ["✋", "🛑"]);
    assert!(is_agent_stop_reaction("✋"));
    assert!(is_agent_stop_reaction("✋\u{1F3FD}"));
    assert!(is_agent_stop_reaction("🛑\u{FE0F}"));
    assert!(!is_agent_stop_reaction("👍"));
    assert!(!is_agent_stop_reaction("✋✋"));
    assert_eq!(reaction_key("✋\u{1F3FB}\u{FE0F}"), "✋");
}
