//! Wire framing and size limit acceptance tests.

use crate::codec::{self, BODY_LIMIT, CLIENT_TAGS_LIMIT, LineBuffer, SERVER_TAGS_LIMIT};
use crate::proto::Message;

// qual:allow(test_quality, no_sut) reason: "Checks the three public wire limit constants directly."
#[test]
fn c01_limits_match_the_specifications() {
    assert_eq!(BODY_LIMIT, 512);
    assert_eq!(CLIENT_TAGS_LIMIT, 4096);
    assert_eq!(SERVER_TAGS_LIMIT, 8191);
}

#[test]
fn c02_frames_crlf_and_bare_lf_and_keeps_partial_input() {
    let mut buffer = LineBuffer::default();
    buffer.extend(b"NOTICE * :one\r\nNOTICE * :two\nNOT");
    assert_eq!(buffer.next_line().ok(), Some(Some("NOTICE * :one".into())));
    assert_eq!(buffer.next_line().ok(), Some(Some("NOTICE * :two".into())));
    assert_eq!(buffer.next_line().ok(), Some(None));
    buffer.extend(b"ICE * :three\r\n");
    assert_eq!(
        buffer.next_line().ok(),
        Some(Some("NOTICE * :three".into()))
    );
    assert_eq!(buffer.next_line().ok(), Some(None));
}

#[test]
fn c03_a_crlf_split_across_reads_is_one_terminator() {
    let mut buffer = LineBuffer::default();
    buffer.extend(b"PING :x\r");
    assert_eq!(buffer.next_line().ok(), Some(None));
    buffer.extend(b"\n");
    assert_eq!(buffer.next_line().ok(), Some(Some("PING :x".into())));
}

#[test]
fn c04_invalid_utf8_becomes_replacement_characters() {
    let mut buffer = LineBuffer::default();
    buffer.extend(b":a!b@c PRIVMSG #x :na\xefve\r\n");
    assert_eq!(
        buffer.next_line().ok(),
        Some(Some(":a!b@c PRIVMSG #x :na\u{fffd}ve".into()))
    );
}

#[test]
fn c05_empty_lines_and_repeated_carriage_returns() {
    let mut buffer = LineBuffer::default();
    buffer.extend(b"\r\nPING :y\r\r\n");
    assert_eq!(buffer.next_line().ok(), Some(Some(String::new())));
    assert_eq!(buffer.next_line().ok(), Some(Some("PING :y".into())));
}

#[test]
fn c06_unterminated_input_errors_only_beyond_a_full_line() {
    let mut buffer = LineBuffer::default();
    buffer.extend(&vec![b'z'; SERVER_TAGS_LIMIT + BODY_LIMIT]);
    assert_eq!(
        buffer.next_line().ok(),
        Some(None),
        "8703 bytes may still end"
    );
    buffer.extend(b"z");
    assert!(
        buffer.next_line().is_err(),
        "8704 bytes without a line feed"
    );
}

#[test]
fn c07_a_terminated_long_line_is_returned_whole() {
    let mut buffer = LineBuffer::default();
    let mut bytes = vec![b'q'; 9000];
    bytes.push(b'\n');
    buffer.extend(&bytes);
    assert_eq!(
        buffer.next_line().ok().flatten().map(|line| line.len()),
        Some(9000)
    );
}

#[test]
fn c08_encode_appends_crlf_after_tags_and_body() {
    let message = Message::new("PRIVMSG", ["#room", "good morning"]).with_tag("label", "a1");
    assert_eq!(
        codec::encode(&message),
        b"@label=a1 PRIVMSG #room :good morning\r\n".to_vec()
    );
    let bare = Message::new("PING", ["t0"]);
    assert_eq!(codec::encode(&bare), b"PING t0\r\n".to_vec());
}

#[test]
fn c09_encode_truncates_the_body_at_a_character_boundary() {
    let message = Message::new("PRIVMSG", ["#x".to_string(), "ü".repeat(300)]);
    let bytes = codec::encode(&message);
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text, format!("PRIVMSG #x {}\r\n", "ü".repeat(249)));
    assert_eq!(text.len(), 511);
}

#[test]
fn c10_encode_keeps_a_tag_section_of_exactly_the_client_limit() {
    // "@k=" (3) + value + " " must total 4096 bytes.
    let value = "v".repeat(CLIENT_TAGS_LIMIT - 4);
    let message = Message::new("TAGMSG", ["#x"]).with_tag("k", &value);
    let text = String::from_utf8(codec::encode(&message)).unwrap();
    let (tags, rest) = text.split_once(' ').unwrap();
    assert_eq!(tags.len() + 1, CLIENT_TAGS_LIMIT);
    assert_eq!(rest, "TAGMSG #x\r\n");
}

#[test]
fn c11_encode_truncates_an_oversized_tag_section() {
    let value = "w".repeat(CLIENT_TAGS_LIMIT + 100);
    let message = Message::new("TAGMSG", ["#x"]).with_tag("k", &value);
    let text = String::from_utf8(codec::encode(&message)).unwrap();
    let (tags, rest) = text.split_once(' ').unwrap();
    assert_eq!(tags.len(), CLIENT_TAGS_LIMIT - 1);
    assert!(tags.starts_with("@k=www"));
    assert_eq!(rest, "TAGMSG #x\r\n");
}
