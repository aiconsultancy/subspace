//! Attachment sends fit one IRC message or retain the composer for retry.
use super::super::completion::{composer, text};
use super::tray::{BATCHING, answer, posts, send, status};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn a_long_request_and_both_uploads_share_one_irc_batch(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let draft = format!(
        "@captain review these files. {}",
        "Context. ".repeat(90).trim()
    );
    let (lines, _api) = send(&view, &input, (&["plan.md", "requirements.md"], &draft), cx);
    assert_eq!(lines[0], draft);
    let keys = subspace_api_client::chat_uploads::references(&lines[1..].join(" "));
    assert_eq!(keys.len(), 2);

    let wire = answer(&view, BATCHING, cx);
    assert_eq!(
        wire.first().unwrap(),
        "BATCH +subspace1 draft/multiline #ship-galactica"
    );
    assert_eq!(wire.last().unwrap(), "BATCH -subspace1");
    let body = &wire[1..wire.len() - 1];
    assert!(body.len() > 2, "the long draft spans physical lines");
    assert!(body.iter().all(|line| line.starts_with("@batch=subspace1")));
    let joined = body.join("\n");
    assert!(joined.contains("@captain review these files."));
    for key in keys {
        assert!(joined.contains(&key), "missing reference {key}");
    }
}

#[gpui_kit::test]
fn a_server_without_multiline_refuses_a_long_attachment_send(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let draft = "context ".repeat(90).trim().to_owned();
    let (_, (mock, _folder)) = send(&view, &input, (&["plan.md"], &draft), cx);
    let wire = answer(&view, "echo-message message-tags", cx);
    assert!(
        wire.is_empty(),
        "neither the draft nor the reference goes out"
    );
    assert_eq!(text(&input, cx), draft);
    assert_eq!(posts(&mock, "plan.md"), 1);
    assert_eq!(view.read_with(cx, |view, _| view.tray.items.len()), 1);
    assert!(status(&input, cx).contains("together"));
}
