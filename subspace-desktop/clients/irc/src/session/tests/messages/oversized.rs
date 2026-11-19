use super::{ALL_CAPS, LineKind, PLAIN_CAPS, Target, in_channel, input, lines};
use crate::{check_draft, check_inline_text};

#[test]
fn oversized_drafts_never_queue_wire_messages_or_local_echoes() {
    let markdown = format!("```md\n{}```\n\n", "  Café\n".repeat(597));
    assert_eq!(markdown.lines().count(), 600);
    for caps in [ALL_CAPS, PLAIN_CAPS] {
        let mut session = in_channel(caps);
        for (target, draft) in [
            (Target::Channel("#c".into()), markdown.clone()),
            (Target::Query("bob".into()), "🚀".repeat(1025)),
            (Target::Server, format!("/query bob {markdown}")),
            (Target::Server, format!("/quote PRIVMSG bob :{markdown}")),
        ] {
            let out = input(&mut session, target, &draft);
            assert!(out.send.is_empty());
            assert_eq!(out.events.len(), 1, "only a local error");
            assert_eq!(lines(&out)[0].kind, LineKind::Error);
        }
    }
}

#[test]
fn the_inline_limit_counts_utf8_bytes_and_accepts_exact_boundaries() {
    for batched in [false, true] {
        assert!(check_inline_text(&"é".repeat(2048), batched).is_ok());
        assert!(check_inline_text(&"é".repeat(2049), batched).is_err());
    }
    assert!(check_inline_text(&"x\n".repeat(10), false).is_ok());
    assert!(check_inline_text(&"x\n".repeat(11), false).is_err());
    assert!(check_inline_text(&"x\n".repeat(100), true).is_ok());
    assert!(check_inline_text(&"x\n".repeat(101), true).is_err());
}

#[test]
fn drafts_use_the_batched_limit_only_for_plain_text_on_a_batching_connection() {
    let plain = "line\n".repeat(25);
    assert!(check_draft(&plain, true).is_ok());
    assert!(check_draft(&plain, false).is_err());
    assert!(check_draft(&format!("/msg bob {plain}"), true).is_err());
    assert!(check_draft(&format!("/quote PRIVMSG bob :{plain}"), true).is_err());
}

#[test]
fn twenty_five_lines_go_as_one_batch_where_the_server_batches_them() {
    let draft = "line\n".repeat(24) + "last";
    let mut session = in_channel(ALL_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), &draft);
    let commands: Vec<&str> = out.send.iter().map(|m| m.command.as_str()).collect();
    assert_eq!(commands.first(), Some(&"BATCH"));
    assert_eq!(commands.last(), Some(&"BATCH"));
    assert_eq!(commands.iter().filter(|c| **c == "PRIVMSG").count(), 25);
    assert!(lines(&out).iter().all(|line| line.kind != LineKind::Error));

    let mut session = in_channel(PLAIN_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), &draft);
    assert!(
        out.send.is_empty(),
        "no batching, so the 10-line limit applies"
    );
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}

fn commands(out: &super::super::Output) -> Vec<&str> {
    out.send.iter().map(|m| m.command.as_str()).collect()
}

fn count(commands: &[&str], name: &str) -> usize {
    commands.iter().filter(|c| **c == name).count()
}

#[test]
fn the_batched_limit_is_exact_on_a_batching_server() {
    let mut session = in_channel(ALL_CAPS);
    let out = input(
        &mut session,
        Target::Channel("#c".into()),
        &"line\n".repeat(100),
    );
    assert_eq!(count(&commands(&out), "BATCH"), 2, "one batch");
    assert_eq!(count(&commands(&out), "PRIVMSG"), 100);
    let out = input(
        &mut session,
        Target::Channel("#c".into()),
        &"line\n".repeat(101),
    );
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}

#[test]
fn commands_keep_ten_lines_and_double_slash_text_is_plain() {
    let mut session = in_channel(ALL_CAPS);
    let lines25 = "line\n".repeat(25);
    for draft in [
        format!("/msg bob {lines25}"),
        format!("/quote PRIVMSG bob :{lines25}"),
    ] {
        let out = input(&mut session, Target::Channel("#c".into()), &draft);
        assert!(out.send.is_empty(), "{draft:.12} is a command");
        assert_eq!(lines(&out)[0].kind, LineKind::Error);
    }
    let out = input(
        &mut session,
        Target::Channel("#c".into()),
        &format!("//{lines25}"),
    );
    assert_eq!(count(&commands(&out), "PRIVMSG"), 25);
}

/// Sends 25 short lines to `#c` on a server whose batches hold `max_lines`.
fn twenty_five_lines(max_lines: usize) -> super::super::Output {
    let caps = ALL_CAPS.replace("max-lines=100", &format!("max-lines={max_lines}"));
    let mut session = in_channel(&caps);
    input(
        &mut session,
        Target::Channel("#c".into()),
        &"line\n".repeat(25),
    )
}

#[test]
fn a_message_too_long_for_one_batch_goes_as_several() {
    let out = twenty_five_lines(20);
    assert_eq!(count(&commands(&out), "BATCH"), 4, "two batches");
    assert_eq!(count(&commands(&out), "PRIVMSG"), 25);

    // On the Subspace Ergo server, a long line split into pieces pushes 99 lines past one batch.
    let mut session = in_channel(ALL_CAPS);
    let draft = "short\n".repeat(98) + &"x".repeat(1000);
    let out = input(&mut session, Target::Channel("#c".into()), &draft);
    assert!(lines(&out).iter().all(|line| line.kind != LineKind::Error));
    assert!(
        count(&commands(&out), "PRIVMSG") > 99,
        "every line and piece is sent"
    );
}

#[test]
fn tiny_batches_and_long_actions_are_refused_not_sent_line_by_line() {
    let out = twenty_five_lines(1);
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);

    let mut session = in_channel(ALL_CAPS);
    let out = super::command(
        &mut session,
        super::Command::Say {
            buffer: super::buffer(Target::Channel("#c".into())),
            text: "wave\n".repeat(11),
            action: true,
        },
    );
    assert!(out.send.is_empty());
    assert_eq!(
        lines(&out)[0].text,
        "Actions and notices can be up to 10 lines."
    );
}

#[test]
fn connection_batches_reads_the_enabled_cap_by_name() {
    assert!(crate::connection_batches(&[
        "batch".into(),
        "draft/multiline".into()
    ]));
    assert!(!crate::connection_batches(&[
        "batch".into(),
        "draft/chathistory".into()
    ]));
}
