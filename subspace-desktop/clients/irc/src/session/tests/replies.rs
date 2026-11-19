use super::{ALL_CAPS, PLAIN_CAPS, buffer, command, feed, in_channel, lines, sent};
use crate::model::{Command, Event, LineKind, Target};

#[test]
fn a_buffer_parent_index_reads_loaded_lines_once_for_many_resolutions() {
    use crate::ThreadRoots;
    use std::cell::Cell;

    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=root :alice!a@h PRIVMSG #c :root",
            "@msgid=middle;+draft/reply=root :alice!a@h PRIVMSG #c :middle",
            "@msgid=leaf;+draft/reply=middle :alice!a@h PRIVMSG #c :leaf",
        ],
    );
    let held = lines(&out);
    let reads = Cell::new(0);
    let loaded = held.iter().copied().inspect(|_| reads.set(reads.get() + 1));
    let roots = ThreadRoots::new(&held[0].buffer, loaded);
    for _ in 0..100 {
        assert_eq!(roots.root(held[2]), Some("root"));
        assert_eq!(roots.root(held[1]), Some("root"));
    }
    assert_eq!(reads.get(), held.len(), "one parent-map pass per buffer");
}

#[test]
fn reply_tags_are_read_live_for_messages_notices_actions_and_queries() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=r;+draft/reply=parent :alice!a@h PRIVMSG #c :reply",
            "@+draft/reply=parent :alice!a@h NOTICE #c :notice",
            "@+draft/reply=parent :alice!a@h PRIVMSG #c :\u{1}ACTION waves\u{1}",
            "@+draft/reply=private :bob!b@h PRIVMSG subspace :private reply",
            "@+draft/reply=private :bob!b@h NOTICE subspace :private notice",
            "@+draft/reply :alice!a@h PRIVMSG #c :empty tag",
        ],
    );
    let held = lines(&out);
    assert_eq!(held[0].reply_to.as_deref(), Some("parent"));
    assert_eq!(held[1].reply_to.as_deref(), Some("parent"));
    assert_eq!(held[2].reply_to.as_deref(), Some("parent"));
    assert_eq!(held[2].kind, LineKind::Action);
    for line in &held[3..5] {
        assert_eq!(line.reply_to.as_deref(), Some("private"));
        assert_eq!(line.buffer, buffer(Target::Query("bob".into())));
    }
    assert_eq!(held[5].reply_to, None);
}

#[test]
fn history_and_nested_multiline_keep_the_opening_batch_reply_tag() {
    let mut session = in_channel(ALL_CAPS);
    for target in ["#c", "bob"] {
        let out = feed(
            &mut session,
            &[
                &format!(":ergo.test BATCH +h draft/chathistory {target}"),
                &format!("@batch=h;+draft/reply=root :bob!b@h PRIVMSG {target} :reply"),
                &format!("@batch=h;+draft/reply=root :bob!b@h NOTICE {target} :notice"),
                &format!(
                    "@batch=h;msgid=ml;+draft/reply=root :bob!b@h BATCH +m draft/multiline {target}"
                ),
                &format!("@batch=m;+draft/reply=wrong :bob!b@h PRIVMSG {target} :one"),
                &format!("@batch=m :bob!b@h PRIVMSG {target} :two"),
                ":bob!b@h BATCH -m",
                ":ergo.test BATCH -h",
            ],
        );
        let page = out
            .events
            .iter()
            .find_map(|event| match event {
                Event::History { lines, .. } => Some(lines),
                _ => None,
            })
            .unwrap();
        assert_eq!(page.len(), 3);
        assert!(
            page.iter()
                .all(|line| line.reply_to.as_deref() == Some("root"))
        );
        assert_eq!(page[2].text, "one\ntwo");
        assert_eq!(page[2].id.as_deref(), Some("ml"));
    }
}

#[test]
fn live_multiline_uses_only_the_opening_reply_tag() {
    let mut session = in_channel(ALL_CAPS);
    for opening in [";+draft/reply=root", ""] {
        let out = feed(
            &mut session,
            &[
                &format!("@msgid=ml{opening} :bob!b@h BATCH +m draft/multiline #c"),
                "@batch=m;+draft/reply=wrong :bob!b@h NOTICE #c :one",
                "@batch=m :bob!b@h NOTICE #c :two",
                ":bob!b@h BATCH -m",
            ],
        );
        assert_eq!(
            lines(&out)[0].reply_to.as_deref(),
            (!opening.is_empty()).then_some("root")
        );
        assert_eq!(lines(&out)[0].kind, LineKind::Notice);
    }
}

fn reply(target: Target, id: &str, text: &str) -> Command {
    Command::Reply {
        buffer: buffer(target),
        msgid: id.into(),
        text: text.into(),
    }
}

#[test]
fn reply_send_escapes_tag_values_and_keeps_text_literal() {
    let mut session = in_channel(ALL_CAPS);
    let out = command(
        &mut session,
        reply(Target::Channel("#c".into()), "a; b\\c\r\n", "/quit now"),
    );
    assert_eq!(
        sent(&out),
        [r"@+draft/reply=a\:\sb\\c\r\n PRIVMSG #c :/quit now"]
    );
    assert!(lines(&out).is_empty());
}

#[test]
fn multiline_replies_tag_only_the_opening_batch_and_fallback_tags_each_piece() {
    let target = Target::Query("bob".into());
    let mut session = in_channel(ALL_CAPS);
    let out = command(&mut session, reply(target.clone(), "root", "one\ntwo"));
    assert_eq!(
        sent(&out),
        [
            "@+draft/reply=root BATCH +subspace1 draft/multiline bob",
            "@batch=subspace1 PRIVMSG bob one",
            "@batch=subspace1 PRIVMSG bob two",
            "BATCH -subspace1",
        ]
    );
    let mut session = in_channel(PLAIN_CAPS);
    let out = command(&mut session, reply(target, "root", "one\ntwo"));
    assert_eq!(
        sent(&out),
        [
            "@+draft/reply=root PRIVMSG bob one",
            "@+draft/reply=root PRIVMSG bob two"
        ]
    );
    assert!(
        lines(&out)
            .iter()
            .all(|line| line.own && line.reply_to.as_deref() == Some("root"))
    );
}

#[test]
fn replies_require_message_tags_and_a_nonempty_id() {
    for (caps, id) in [("echo-message", "root"), (ALL_CAPS, "")] {
        let mut session = in_channel(caps);
        let out = command(
            &mut session,
            reply(Target::Channel("#c".into()), id, "reply"),
        );
        assert!(out.send.is_empty());
        assert!(lines(&out).iter().all(|line| line.kind == LineKind::Error));
    }
}

#[test]
fn replying_to_a_reply_uses_the_loaded_root_and_is_capability_gated() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=child;+draft/reply=middle :bob!b@h PRIVMSG #c :child",
            "@msgid=middle;+draft/reply=root :bob!b@h PRIVMSG #c :middle",
            "@msgid=root :alice!a@h PRIVMSG #c :parent",
        ],
    );
    let held = lines(&out);
    assert_eq!(held[0].thread_root(&held), Some("root"));
    assert_eq!(
        held[0].reply("answer", &held, true),
        Some(reply(Target::Channel("#c".into()), "root", "answer"))
    );
    assert_eq!(held[0].reply("answer", &held, false), None);
    assert_eq!(held[0].thread_root(&held[..1]), Some("middle"));
    let json = serde_json::to_string(held[0]).unwrap();
    let restored: crate::Line = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.reply_to.as_deref(), Some("middle"));
}

#[test]
fn roots_are_scoped_to_the_buffer_and_cycles_cannot_loop() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=a;+draft/reply=b :bob!b@h PRIVMSG #c :a",
            "@msgid=b;+draft/reply=a :bob!b@h PRIVMSG #c :b",
            "@msgid=q;+draft/reply=b :bob!b@h PRIVMSG subspace :query",
            ":bob!b@h PRIVMSG subspace :no id",
        ],
    );
    let held = lines(&out);
    assert_eq!(held[0].thread_root(&held), None);
    assert_eq!(held[2].thread_root(&held), Some("b"));
    assert_eq!(held[3].reply("answer", &held, true), None);
}

#[test]
fn reply_send_rejects_invalid_ids_and_oversized_text_without_echo() {
    let mut session = in_channel(PLAIN_CAPS);
    for (id, text) in [
        ("\0", "hello".into()),
        (&";".repeat(4096), "hello".into()),
        ("root", "x".repeat(4097)),
    ] {
        let out = command(&mut session, reply(Target::Channel("#c".into()), id, &text));
        assert!(out.send.is_empty());
        assert_eq!(lines(&out).len(), 1);
        assert_eq!(lines(&out)[0].kind, LineKind::Error);
    }
}

#[test]
fn untagged_stored_messages_still_deserialize_and_private_replies_resolve() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=a :bob!b@h PRIVMSG subspace :parent",
            "@msgid=b;+draft/reply=a :bob!b@h PRIVMSG subspace :child",
        ],
    );
    let held = lines(&out);
    assert_eq!(
        held[1].reply("answer", &held, true),
        Some(reply(Target::Query("bob".into()), "a", "answer"))
    );
    let json = serde_json::to_string(held[0]).unwrap();
    assert!(!json.contains("reply_to"));
    let restored: crate::Line = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, *held[0]);
}

#[test]
fn a_prepared_reply_is_rejected_when_message_tags_is_withdrawn() {
    let mut session = in_channel(ALL_CAPS);
    let received = feed(&mut session, &["@msgid=root :bob!b@h PRIVMSG #c :parent"]);
    let held = lines(&received);
    let prepared = held[0].reply("answer", &held, true).unwrap();
    feed(&mut session, &[":ergo.test CAP subspace DEL :message-tags"]);
    let out = command(&mut session, prepared);
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}

#[test]
fn reply_tag_size_boundary_is_accepted_without_truncating_the_id() {
    // 15 bytes for @+draft/reply= and the separator; 4081 for the ID.
    let id = "x".repeat(4081);
    let mut session = in_channel(ALL_CAPS);
    let out = command(
        &mut session,
        reply(Target::Channel("#c".into()), &id, "answer"),
    );
    assert_eq!(out.send.len(), 1);
    let encoded = String::from_utf8(crate::codec::encode(&out.send[0])).unwrap();
    let decoded = crate::proto::parse(encoded.trim_end()).unwrap();
    assert_eq!(decoded.tag("+draft/reply"), Some(id.as_str()));
    let out = command(
        &mut session,
        reply(Target::Channel("#c".into()), &(id + "x"), "answer"),
    );
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}
