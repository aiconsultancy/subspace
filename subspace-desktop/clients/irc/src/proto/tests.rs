//! Protocol round trips with independently chosen wire input.

use super::{parse, serialise};

#[test]
fn normalises_separators_without_changing_trailing_text() {
    let message = parse("@z=last;a=a\\sb  :relay!service  notice  #docks :  still here  ").unwrap();
    let wire = serialise(&message);
    assert_eq!(
        wire,
        "@a=a\\sb;z=last :relay!service NOTICE #docks :  still here  "
    );
    assert_eq!(parse(&wire).unwrap(), message);
}

#[test]
fn separates_a_bridged_nick_from_its_user_and_host() {
    let wire = ":koi:river@bridge!relay@edge.example PRIVMSG #docks ::welcome";
    let message = parse(wire).unwrap();
    assert_eq!(message.nick(), Some("koi:river@bridge"));
    assert_eq!(message.trailing(), ":welcome");
    assert_eq!(serialise(&message), wire);
}

#[test]
fn ignores_a_tag_with_no_key() {
    let message = parse("@=;=value;label=ok PING water").unwrap();
    assert_eq!(serialise(&message), "@label=ok PING water");
}
