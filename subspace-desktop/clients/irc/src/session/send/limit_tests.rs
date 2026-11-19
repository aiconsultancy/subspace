use super::{MAX_INLINE_BYTES, check_draft, draft_line_limit};

#[test]
fn counter_limits_and_validation_share_commands_literals_and_connection_caps() {
    for (prefix, batched, lines) in [
        ("", false, 10),
        ("", true, 100),
        ("/quote ", true, 10),
        ("//literal ", true, 100),
    ] {
        let at_limit = format!("{prefix}{}", vec!["line"; lines].join("\n"));
        assert_eq!(draft_line_limit(&at_limit, batched), lines);
        assert!(check_draft(&at_limit, batched).is_ok());
        assert!(check_draft(&format!("{at_limit}\nextra"), batched).is_err());
        let bytes = format!("{prefix}{}", "a".repeat(MAX_INLINE_BYTES - prefix.len()));
        assert!(check_draft(&bytes, batched).is_ok());
        assert!(check_draft(&format!("{bytes}a"), batched).is_err());
    }
}

#[test]
fn message_budget_matches_the_split_budget_and_falls_back_to_the_nick() {
    assert_eq!(
        super::message_budget("nick", "", "#chan"),
        super::budget("PRIVMSG", "nick", "nick", "#chan", 0)
    );
    assert_eq!(
        super::message_budget("nick", "user", "#chan"),
        super::budget("PRIVMSG", "nick", "user", "#chan", 0)
    );
}
