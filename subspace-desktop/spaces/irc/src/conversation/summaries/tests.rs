use super::{Summaries, identity, position};
use crate::{
    fixtures::{channel, message},
    state::Entry,
};
use gpui_kit::TestAppContext;

fn entry(seq: i64, id: Option<&str>) -> Entry {
    let mut line = message(&channel("#subspace"), "bob", "TST-12", 1);
    line.id = id.map(str::to_owned);
    Entry {
        seq,
        line,
        highlight: false,
    }
}

#[test]
fn position_compares_message_ids_and_untagged_sequences() {
    let entries = [entry(2, Some("server:message")), entry(3, None)];
    assert_eq!(position(entries.iter(), "msgid:server:message"), Some(0));
    assert_eq!(position(entries.iter(), "message:3"), Some(1));
    assert_eq!(position(entries.iter(), "message:2"), None);
    assert_eq!(position(entries.iter(), "msgid:missing"), None);
    assert_eq!(position(entries.iter(), "message:invalid"), None);
    assert_eq!(position(entries.iter(), "unknown:3"), None);
    let rebuilt = [entry(-20, Some("server:message")), entry(-3, None)];
    assert_eq!(position(rebuilt.iter(), "msgid:server:message"), Some(0));
    assert_eq!(position(rebuilt.iter(), "message:-3"), Some(1));
}

#[gpui_kit::test]
fn message_identity_survives_sequence_changes_and_dropped_rows_are_pruned(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let tagged = entry(2, Some("server-message"));
        let untagged = entry(3, None);
        let mut summaries = Summaries::default();
        summaries.toggle(identity(&tagged), "TST-12", cx);
        summaries.toggle(identity(&untagged), "TST-18", cx);
        let rebuilt = entry(-20, Some("server-message"));
        assert_eq!(identity(&rebuilt), identity(&tagged));
        assert_eq!(identity(&untagged), "message:3");
        summaries.retain([&rebuilt, &untagged].into_iter());
        assert_eq!(
            summaries.panel(&identity(&rebuilt)).unwrap().issue,
            "TST-12"
        );
        assert!(
            summaries
                .panel(&identity(&rebuilt))
                .unwrap()
                .motion
                .is_open()
        );
        summaries.retain([&untagged].into_iter());
        assert!(summaries.panel(&identity(&rebuilt)).is_none());
        assert!(summaries.panel(&identity(&untagged)).is_some());
        summaries.retain(std::iter::empty());
        assert!(summaries.panels.is_empty());
    });
}
