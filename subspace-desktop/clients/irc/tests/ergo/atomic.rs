use super::{Client, Event, Line, LineKind, Target, join_both, registered_client, server, suffix};

fn messages_from<'a>(client: &'a Client, start: usize, nick: &str) -> Vec<&'a Line> {
    client.events[start..]
        .iter()
        .filter_map(|event| match event {
            Event::Line(line)
                if line.kind == LineKind::Privmsg && line.from.as_deref() == Some(nick) =>
            {
                Some(line)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn composed_caption_and_later_upload_are_two_complete_messages() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the atomic Ergo test");
        return;
    };
    let suffix = suffix();
    let here = Target::Channel(format!("#atomic-{suffix}"));
    let mut sender = registered_client("atomic-a", &format!("ata{suffix}"), &server);
    let mut receiver = registered_client("atomic-b", &format!("atb{suffix}"), &server);
    let Target::Channel(channel) = &here else {
        unreachable!()
    };
    join_both(&mut sender, &mut receiver, channel);
    assert!(sender.events.iter().any(|event| matches!(event, Event::Capabilities { enabled, .. }
        if enabled.iter().any(|cap| cap == "draft/multiline") && enabled.iter().any(|cap| cap == "batch"))));
    let caption = include_str!("../fixtures/atomic-caption.txt").trim_end_matches('\n');
    let upload = "UPL-AAAAAAAAAAAAAAAAAAAAAA — screenshot.png (1234 bytes)";
    let spaces = format!("before{}after", " ".repeat(900));
    let start = receiver.mark();
    sender.input(here.clone(), caption);
    sender.input(here.clone(), upload);
    sender.input(here.clone(), &spaces);
    sender.input(here, "atomic fixture complete");
    receiver.wait_line(start, "ordered end marker", |line| {
        line.text == "atomic fixture complete"
    });
    let messages = messages_from(&receiver, start, &sender.config.nickname);
    assert_eq!(
        messages
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        [caption, upload, &spaces, "atomic fixture complete"]
    );
    assert!(
        messages[0]
            .text
            .ends_with("I am about to send a screenshot so do not respond until I have")
    );
    assert!(
        messages
            .iter()
            .all(|line| line.id.is_some()
                && line.account.as_deref() == Some(&sender.config.nickname))
    );
    assert_ne!(messages[0].id, messages[1].id);
}
