//! Message parsing, serialisation and CTCP acceptance tests.

use crate::proto::{self, Message, ctcp};

fn user(message: &Message) -> (Option<&str>, Option<String>) {
    (
        message.nick(),
        message.source.as_ref().map(|_| proto::serialise(message)),
    )
}

#[test]
fn p01_parses_tags_source_command_and_params() {
    let message = proto::parse(
        "@label=x7;+example.org/flag :carol!c@host.example PRIVMSG #ops :status check",
    )
    .unwrap();
    assert_eq!(message.tag("label"), Some("x7"));
    assert_eq!(message.tag("+example.org/flag"), Some(""));
    assert_eq!(message.tag("missing"), None);
    assert_eq!(message.nick(), Some("carol"));
    assert_eq!(message.command, "PRIVMSG");
    assert_eq!(message.params, ["#ops", "status check"]);
    assert_eq!(message.param(0), "#ops");
    assert_eq!(message.param(7), "");
    assert_eq!(message.trailing(), "status check");
}

#[test]
fn p02_trailing_keeps_spaces_and_colons_and_separators_collapse() {
    let message = proto::parse(":srv.example 372 me :-  welcome :) to  the server ").unwrap();
    assert_eq!(message.params, ["me", "-  welcome :) to  the server "]);
    let message = proto::parse("MODE   #ops   +v    dave").unwrap();
    assert_eq!(message.params, ["#ops", "+v", "dave"]);
    let message = proto::parse("@a=1   :x.example   PING   :tok").unwrap();
    assert_eq!(message.command, "PING");
    assert_eq!(message.params, ["tok"]);
}

#[test]
fn p03_commands_are_upper_cased_and_numerics_kept() {
    assert_eq!(proto::parse("notice #a :hi").unwrap().command, "NOTICE");
    let numeric =
        proto::parse(":srv.example 352 me #ops ~u h.example srv.example dave H :0 Dave").unwrap();
    assert_eq!(numeric.command, "352");
    assert_eq!(
        numeric.params,
        [
            "me",
            "#ops",
            "~u",
            "h.example",
            "srv.example",
            "dave",
            "H",
            "0 Dave"
        ]
    );
}

#[test]
fn p04_rejects_lines_without_a_valid_command() {
    for line in [
        "",
        "   ",
        "@tag=1",
        ":src.example",
        "@tag=1 :src.example",
        "9 y",
        "0042 y",
        "12345 y",
        "NOT_A_CMD y",
        "CMD2 y",
        " PING x",
        "ü x",
    ] {
        assert!(proto::parse(line).is_err(), "{line:?}");
    }
    let error = proto::parse("0042 y").unwrap_err().to_string();
    assert!(error.contains("0042 y"), "{error}");
}

#[test]
fn p05_line_endings_empty_trailing_and_no_params() {
    assert_eq!(proto::parse("PING :abc\r\n").unwrap().params, ["abc"]);
    assert_eq!(proto::parse("PING :abc\n").unwrap().params, ["abc"]);
    assert_eq!(proto::parse("TOPIC #a :").unwrap().params, ["#a", ""]);
    let quit = proto::parse("QUIT").unwrap();
    assert!(quit.params.is_empty());
    assert_eq!(quit.trailing(), "");
    assert!(quit.source.is_none());
    assert_eq!(quit.nick(), None);
}

#[test]
fn p06_unescapes_tag_values() {
    let message = proto::parse(
        r"@a=one\stwo;b=semi\:colon;c=back\\slash;d=cr\rlf\n;e=\q;f=trail\;g=;h :s.example TAGMSG #x",
    )
    .unwrap();
    assert_eq!(message.tag("a"), Some("one two"));
    assert_eq!(message.tag("b"), Some("semi;colon"));
    assert_eq!(message.tag("c"), Some(r"back\slash"));
    assert_eq!(message.tag("d"), Some("cr\rlf\n"));
    assert_eq!(message.tag("e"), Some("q"));
    assert_eq!(message.tag("f"), Some("trail"));
    assert_eq!(message.tag("g"), Some(""));
    assert_eq!(message.tag("h"), Some(""));
}

#[test]
fn p07_the_last_duplicate_tag_wins_and_empty_items_are_skipped() {
    let message = proto::parse("@k=1;;k=2; PING x").unwrap();
    assert_eq!(message.tag("k"), Some("2"));
    assert_eq!(proto::serialise(&message), "@k=2 PING x");
}

#[test]
fn p08_classifies_sources() {
    let server = proto::parse(":services.example.org NOTICE me :hi").unwrap();
    assert_eq!(
        user(&server),
        (None, Some(":services.example.org NOTICE me hi".into()))
    );
    let ipv6 = proto::parse(":2001:db8::7 NOTICE me :hi").unwrap();
    assert_eq!(ipv6.nick(), None);
    let bare = proto::parse(":Helper NOTICE me :hi").unwrap();
    assert_eq!(bare.nick(), Some("Helper"));
    let full = proto::parse(":ann!~ann@203.0.113.9 JOIN #x").unwrap();
    assert_eq!(full.nick(), Some("ann"));
    assert_eq!(proto::serialise(&full), ":ann!~ann@203.0.113.9 JOIN #x");
    let bridged = proto::parse(":bridge[m]@chat.example!relay@gw.example PRIVMSG #x :y").unwrap();
    assert_eq!(bridged.nick(), Some("bridge[m]@chat.example"));
    let ident = proto::parse(":ann!ident JOIN #x").unwrap();
    assert_eq!(ident.nick(), Some("ann"));
    assert_eq!(proto::serialise(&ident), ":ann!ident JOIN #x");
    let at_only = proto::parse(":ann@hostname NOTICE me :hi").unwrap();
    assert_eq!(at_only.nick(), Some("ann@hostname"));
    let at_dotted = proto::parse(":ann@host.example NOTICE me :hi").unwrap();
    assert_eq!(at_dotted.nick(), None, "a bare name with a dot is a server");
}

// ---- proto::Message -------------------------------------------------------

#[test]
fn m01_new_upper_cases_and_with_tag_replaces() {
    let message = Message::new("part", ["#a", "x"])
        .with_tag("k", "1")
        .with_tag("k", "2");
    assert_eq!(message.command, "PART");
    assert!(message.source.is_none());
    assert_eq!(message.tag("k"), Some("2"));
    let owned = Message::new("JOIN", vec!["#a".to_string()]);
    assert_eq!(owned.params, ["#a"]);
    let none = Message::new("LIST", Vec::<String>::new());
    assert!(none.params.is_empty());
}

// ---- proto::serialise -----------------------------------------------------

#[test]
fn s01_adds_a_colon_only_when_the_last_parameter_needs_one() {
    let line = |params: &[&str]| proto::serialise(&Message::new("PRIVMSG", params.iter().copied()));
    assert_eq!(line(&["#a", "evening"]), "PRIVMSG #a evening");
    assert_eq!(line(&["#a", "good evening"]), "PRIVMSG #a :good evening");
    assert_eq!(line(&["#a", ""]), "PRIVMSG #a :");
    assert_eq!(line(&["#a", ":)"]), "PRIVMSG #a ::)");
    assert_eq!(line(&[]), "PRIVMSG");
    assert_eq!(
        proto::serialise(&Message::new("CHATHISTORY", ["LATEST", "bob", "*", "50"])),
        "CHATHISTORY LATEST bob * 50"
    );
}

#[test]
fn s02_sorts_tags_omits_empty_values_and_escapes() {
    let message = Message::new("TAGMSG", ["#a"])
        .with_tag("zeta", "1")
        .with_tag("+alpha", "")
        .with_tag("mid", "x y");
    assert_eq!(
        proto::serialise(&message),
        r"@+alpha;mid=x\sy;zeta=1 TAGMSG #a"
    );
    let escaped = Message::new("TAGMSG", ["#a"]).with_tag("k", "a;b c\\d\r\ne");
    assert_eq!(proto::serialise(&escaped), r"@k=a\:b\sc\\d\r\ne TAGMSG #a");
}

#[test]
fn s03_round_trips_tags_sources_and_parameters() {
    for line in [
        r"@+reply=a\sb;msgid=Zx1;time=2025-03-04T05:06:07.890Z :dave!d@gw.example PRIVMSG #ops :see above",
        ":services.example.org 353 me = #ops :@dave +erin frank",
        "@+typing=active :erin!e@h.example TAGMSG #ops",
        ":frank!f@h.example PART #ops",
    ] {
        let parsed = proto::parse(line).unwrap();
        assert_eq!(proto::serialise(&parsed), line);
        assert_eq!(proto::parse(&proto::serialise(&parsed)).unwrap(), parsed);
    }
}

// ---- proto::ctcp ----------------------------------------------------------

#[test]
fn t01_parses_ctcp_queries() {
    let query = ctcp::parse("\u{1}VERSION\u{1}").unwrap();
    assert_eq!((query.command.as_str(), query.params), ("VERSION", ""));
    let query = ctcp::parse("\u{1}ping 1700000000\u{1}").unwrap();
    assert_eq!(
        (query.command.as_str(), query.params),
        ("PING", "1700000000")
    );
    let query = ctcp::parse("\u{1}ACTION stretches").unwrap();
    assert_eq!(
        (query.command.as_str(), query.params),
        ("ACTION", "stretches")
    );
    let query = ctcp::parse("\u{1}ACTION  looks around \u{1}").unwrap();
    assert_eq!(query.params, " looks around ");
}

#[test]
fn t02_rejects_text_that_is_not_ctcp() {
    for text in [
        "plain text",
        "",
        "\u{1}\u{1}",
        "\u{1}",
        "\u{1} x\u{1}",
        "x\u{1}VERSION\u{1}",
    ] {
        assert!(ctcp::parse(text).is_none(), "{text:?}");
    }
}
