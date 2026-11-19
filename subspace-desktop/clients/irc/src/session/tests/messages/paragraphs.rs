use super::{ALL_CAPS, PLAIN_CAPS, Target, in_channel, input, lines};
use crate::model::{Command, Line};
use crate::proto::{Message, parse, serialise};
use crate::session::Output;
use crate::session::tests::{buffer, command, now};

const MULTILINE: &str = "draft/multiline";
const CONCAT: &str = "draft/multiline-concat";
const REPLY: &str = "+draft/reply";

fn paragraph() -> String {
    format!(
        "captain: {}finish the whole request.",
        "Please read this complete sentence before starting. ".repeat(18)
    )
}

fn send(caps: &str, text: &str) -> Output {
    input(&mut in_channel(caps), Target::Channel("#c".into()), text)
}

fn received_batch(out: &Output) -> Line {
    let mut receiver = in_channel(ALL_CAPS);
    let mut received = Vec::new();
    for (index, message) in out.send.iter().enumerate() {
        let result = receiver.receive(relay(message), now());
        received.extend(lines(&result).into_iter().cloned());
        if index + 1 < out.send.len() {
            assert!(
                received.is_empty(),
                "no partial message before BATCH closes"
            );
        }
    }
    assert_eq!(received.len(), 1);
    received.pop().unwrap()
}

fn relay(message: &Message) -> Message {
    let source = parse(&format!(
        ":fixture1!fixture1@{} PRIVMSG #c example",
        "h".repeat(63)
    ))
    .unwrap()
    .source;
    let mut relay = message.clone();
    relay.source = source;
    if message.command == "BATCH" && message.param(0).starts_with('+') {
        relay = relay
            .with_tag("msgid", "whole")
            .with_tag("account", "alice");
    }
    let wire = serialise(&relay);
    assert_eq!(
        crate::codec::encode(&relay),
        format!("{wire}\r\n").as_bytes(),
        "relayed body and tags fit without truncation"
    );
    parse(&wire).unwrap()
}

fn assert_batch(out: &Output, text: &str) {
    assert_eq!(out.send.first().unwrap().command, "BATCH");
    assert_eq!(out.send.last().unwrap().command, "BATCH");
    assert!(out.send.len() > 3, "multiple physical message pieces");
    for message in &out.send {
        assert_eq!(
            crate::codec::encode(message),
            format!("{}\r\n", serialise(message)).as_bytes()
        );
    }
    let received = received_batch(out);
    assert_eq!(received.text, text);
    assert_eq!(received.id.as_deref(), Some("whole"));
    assert_eq!(received.account.as_deref(), Some("alice"));
}

#[test]
fn ascii_and_utf8_single_paragraphs_arrive_atomically_without_losing_spaces() {
    for text in [
        paragraph(),
        format!("captain: {}done.", "Read café 🚀 with care. ".repeat(35)),
    ] {
        let out = send(ALL_CAPS, &text);
        assert_batch(&out, &text);
        let body = &out.send[1..out.send.len() - 1];
        assert!(body[0].tag(CONCAT).is_none());
        assert!(
            body[1..]
                .iter()
                .all(|message| message.tag(CONCAT) == Some(""))
        );
    }
}

#[test]
fn one_composed_caption_is_complete_before_the_separate_screenshot_message() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/atomic-caption.txt"
    ))
    .trim_end_matches('\n');
    let upload = "UPL-AAAAAAAAAAAAAAAAAAAAAA — screenshot.png (1234 bytes)";
    let caption = send(ALL_CAPS, text);
    assert_batch(&caption, text);
    let flattened = text.replace("\n\n", " ");
    let flattened_caption = send(ALL_CAPS, &flattened);
    assert_batch(&flattened_caption, &flattened);
    assert!(text.ends_with("I am about to send a screenshot so do not respond until I have"));
    let mut receiver = in_channel(ALL_CAPS);
    let mut received = Vec::new();
    for (index, message) in caption.send.iter().enumerate() {
        let out = receiver.receive(relay(message), now());
        received.extend(lines(&out).into_iter().map(|line| line.text.clone()));
        assert_eq!(received.len(), usize::from(index + 1 == caption.send.len()));
    }
    assert_eq!(received, [text]);
    let screenshot = send(ALL_CAPS, upload);
    for message in &screenshot.send {
        let out = receiver.receive(relay(message), now());
        received.extend(lines(&out).into_iter().map(|line| line.text.clone()));
    }
    assert_eq!(received, [text, upload]);
    if let Some(path) = std::env::var_os("SUBSPACE_IRC_WIRE_EVIDENCE") {
        let evidence = serde_json::json!({
            "caption": text, "captionFrames": caption.send.iter().map(serialise).collect::<Vec<_>>(),
            "flattened": flattened, "flattenedFrames": flattened_caption.send.iter().map(serialise).collect::<Vec<_>>(),
            "upload": upload, "uploadFrames": screenshot.send.iter().map(serialise).collect::<Vec<_>>()
        });
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn nonempty_space_only_continuations_preserve_the_complete_text() {
    let text = format!("captain: before{}after", " ".repeat(900));
    let out = send(ALL_CAPS, &text);
    assert_batch(&out, &text);
    assert!(out.send.iter().any(|message| message.tag(CONCAT).is_some()
        && message.trailing().chars().all(|character| character == ' ')));
    assert!(
        out.send
            .iter()
            .filter(|message| message.tag(CONCAT).is_some())
            .all(|message| !message.trailing().is_empty())
    );
}

#[test]
fn unbroken_ascii_and_multibyte_words_fit_the_exact_inline_byte_limit() {
    let prefix = "captain:";
    for unit in ["x", "🚀"] {
        let text = format!(
            "{prefix}{}",
            unit.repeat((crate::MAX_INLINE_BYTES - prefix.len()) / unit.len())
        );
        assert_eq!(text.len(), crate::MAX_INLINE_BYTES);
        assert_batch(&send(ALL_CAPS, &text), &text);
    }
}

#[test]
fn wrapped_lines_and_blank_lines_keep_only_the_original_newlines() {
    let text = format!(
        "{}\n\n{}",
        paragraph(),
        "Second café 🚀 paragraph. ".repeat(25)
    );
    let out = send(ALL_CAPS, &text);
    assert_batch(&out, &text);
    let starts = out
        .send
        .iter()
        .filter(|message| message.command == "PRIVMSG" && message.tag(CONCAT).is_none())
        .count();
    assert_eq!(
        starts, 3,
        "one start for each logical line, including blank"
    );
}

#[test]
fn wrapped_reply_tags_only_the_opening_batch_and_keeps_its_message_id() {
    let text = paragraph();
    let out = command(
        &mut in_channel(ALL_CAPS),
        Command::Reply {
            buffer: buffer(Target::Channel("#c".into())),
            msgid: "root".into(),
            text: text.clone(),
        },
    );
    assert_batch(&out, &text);
    assert_eq!(out.send[0].tag(REPLY), Some("root"));
    assert!(
        out.send[1..]
            .iter()
            .all(|message| message.tag(REPLY).is_none())
    );
    assert_eq!(received_batch(&out).reply_to.as_deref(), Some("root"));
}

#[test]
fn negotiated_byte_and_physical_line_limits_accept_exact_boundaries() {
    let text = paragraph();
    let out = send(ALL_CAPS, &text);
    let physical_lines = out
        .send
        .iter()
        .filter(|message| message.command == "PRIVMSG")
        .count();
    let caps = ALL_CAPS.replace(
        "max-bytes=4096,max-lines=100",
        &format!("max-bytes={},max-lines={physical_lines}", text.len()),
    );
    assert_batch(&send(&caps, &text), &text);
    let baseline = send(PLAIN_CAPS, &text).send;
    for limits in [
        format!("max-bytes={},max-lines=100", text.len() - 1),
        format!("max-bytes=4096,max-lines={}", physical_lines - 1),
        "max-bytes=0,max-lines=100".into(),
        "max-bytes=4096,max-lines=0".into(),
        "max-bytes=4096,max-lines=1".into(),
    ] {
        let caps = ALL_CAPS.replace("max-bytes=4096,max-lines=100", &limits);
        assert_eq!(
            send(&caps, &text).send,
            baseline,
            "legacy fallback for {limits}"
        );
    }
}

#[test]
fn missing_or_unacknowledged_multiline_keeps_the_legacy_standalone_messages() {
    let text = paragraph();
    let baseline = send(PLAIN_CAPS, &text).send;
    assert!(baseline.len() > 1);
    for missing in ["enabled", "offered"] {
        let mut session = in_channel(ALL_CAPS);
        if missing == "enabled" {
            session.caps.enabled.remove(MULTILINE);
        } else {
            session.caps.offered.remove(MULTILINE);
        }
        assert_eq!(
            input(&mut session, Target::Channel("#c".into()), &text).send,
            baseline
        );
    }
}

#[test]
fn short_messages_stay_standalone_and_unvalued_caps_use_the_default_limits() {
    assert_eq!(
        send(ALL_CAPS, "captain: hello").send,
        vec![Message::new("PRIVMSG", ["#c", "captain: hello"])]
    );
    let caps = ALL_CAPS.replace("draft/multiline=max-bytes=4096,max-lines=100", MULTILINE);
    assert_batch(&send(&caps, &paragraph()), &paragraph());
}
