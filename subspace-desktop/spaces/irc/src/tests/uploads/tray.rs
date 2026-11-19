//! The tray against a local API: limits, `SendAll` sends, line packing
//! and clearing once every reference echoes.
use super::super::completion::{composer, text};
use super::connect;
use super::rehearse::{Rehearsed, rehearse};
use crate::conversation::Conversation;
use crate::uploads::outgoing::{self, LIMIT_NOTICE, MAX_ITEMS, Phase};
use base64::Engine as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use sha2::{Digest as _, Sha256};
use subspace_client_irc::{Command, ConnectionStatus, Event};
use subspace_framework::client_test_support::Mock;

/// Echoes each upload's metadata back and answers Check upload with it. A
/// file named `fail…` is refused; `flaky…` is refused on its first try;
/// `gone…` is stored but never found again; `old…` has already expired on
/// its first try.
pub(super) fn server() -> Mock {
    let made = std::sync::Mutex::new(std::collections::HashMap::<String, String>::new());
    Mock::with_bytes(move |request| {
        let json = |body: String| {
            (
                200,
                vec![("Content-Type".into(), "application/json".into())],
                body.into_bytes(),
            )
        };
        if request.path.ends_with("/me") {
            return json(r#"{"userId":"me","kind":"human","displayName":"Fixture"}"#.into());
        }
        let mut made = made.lock().unwrap();
        if request.method == "GET" {
            let key = request.path.rsplit('/').next().unwrap_or_default();
            return match made.get(key) {
                Some(body) => json(body.clone()),
                None => (404, Vec::new(), Vec::new()),
            };
        }
        let body: serde_json::Value = serde_json::from_slice(&request.bytes).unwrap_or_default();
        let name = body["fileName"].as_str().unwrap_or_default();
        if name.starts_with("fail") || (name.starts_with("flaky") && !made.contains_key(name)) {
            made.insert(name.to_owned(), String::new());
            return (413, Vec::new(), Vec::new());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body["base64"].as_str().unwrap_or_default())
            .unwrap_or_default();
        let id = body["uploadId"].as_str().unwrap_or_default();
        let uuid: Vec<u8> = hex::decode(id.replace('-', "")).unwrap_or_default();
        let reference = format!(
            "UPL-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(uuid)
        );
        let expired =
            name.starts_with("old") && made.insert(name.to_owned(), String::new()).is_none();
        let expires = if expired {
            "2026-01-01T00:00:00Z"
        } else {
            "2099-01-01T00:00:00Z"
        };
        let echo = serde_json::json!({
            "id": id, "reference": reference, "fileName": name,
            "contentType": body["contentType"], "sizeBytes": bytes.len(),
            "sha256": hex::encode(Sha256::digest(&bytes)), "uploadedByUserId": "me",
            "createdAt": "2026-11-17T00:00:00Z", "expiresAt": expires,
            "url": "/unused"
        })
        .to_string();
        if !name.starts_with("gone") {
            made.insert(reference, echo.clone());
        }
        json(echo)
    })
}

/// How many times the API was asked to store a file named `name`.
pub(super) fn posts(mock: &Mock, name: &str) -> usize {
    mock.requests()
        .iter()
        .filter(|request| request.method == "POST" && request.path.ends_with("/chat/uploads"))
        .filter(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(&request.bytes).unwrap_or_default();
            body["fileName"] == name
        })
        .count()
}

/// Sets the conversation's network up or down, rejoining on the way up.
pub(super) fn network(view: &Entity<Conversation>, up: bool, cx: &mut VisualTestContext) {
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    let room = view.read_with(cx, |view, _| view.buffer.clone());
    let status = if up {
        ConnectionStatus::Connected {
            nick: "subspace".into(),
        }
    } else {
        ConnectionStatus::Disconnected
    };
    let mut events = vec![Event::Status {
        network: room.network.clone(),
        status,
    }];
    if up {
        events.push(Event::Joined { buffer: room });
    }
    crate::tests::inject(&hub, events, cx);
}

/// Signs in to the session again and reconnects, then presses Enter: an
/// upload from the earlier session waits for Check upload.
pub(super) fn renew_and_resend(view: &Entity<Conversation>, cx: &mut VisualTestContext) {
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    hub.update(cx, |hub, _| hub.uploads.session.test_renew());
    network(view, true, cx);
    submit(view, "", cx);
}

/// The error under the composer, if any.
pub(super) fn status(
    input: &Entity<subspace_framework::dashboard::ChatInput>,
    cx: &mut VisualTestContext,
) -> String {
    input.read_with(cx, |input, _| input.error().unwrap_or_default().to_owned())
}

/// Adds text files named `names` to the tray.
pub(super) fn attach(
    view: &Entity<Conversation>,
    names: &[&str],
    cx: &mut VisualTestContext,
) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    let paths = names
        .iter()
        .map(|name| {
            let path = folder.path().join(name);
            std::fs::write(&path, format!("contents of {name}")).unwrap();
            path
        })
        .collect();
    view.update(cx, |view, cx| outgoing::paths(view, paths, cx));
    cx.run_until_parked();
    folder
}

/// Signs in to a local API, attaches `names`, types `typed` and presses
/// Enter. Returns the lines sent to IRC and the API and files to keep alive.
pub(super) fn send(
    view: &Entity<Conversation>,
    input: &Entity<subspace_framework::dashboard::ChatInput>,
    (names, typed): (&[&str], &str),
    cx: &mut VisualTestContext,
) -> (Vec<String>, (Mock, tempfile::TempDir)) {
    let mock = server();
    connect(view, &mock, cx);
    let folder = attach(view, names, cx);
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_text(typed, window, cx)));
    submit(view, typed, cx);
    (sent(view, cx), (mock, folder))
}

/// Presses Enter with `text`. The test dispatcher runs every background
/// upload to completion on this thread, so one parked run settles the send.
pub(super) fn submit(view: &Entity<Conversation>, text: &str, cx: &mut VisualTestContext) {
    let input = view.read_with(cx, |view, _| view.composer().clone());
    let text = text.to_owned();
    input.update(cx, |_, cx| {
        cx.emit(subspace_framework::dashboard::ChatInputEvent::Submit(text))
    });
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| !matches!(
            view.tray.phase,
            Phase::Uploading { .. }
        )),
        "the send settled"
    );
}

pub(super) fn sent(view: &Entity<Conversation>, cx: &mut VisualTestContext) -> Vec<String> {
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .flat_map(|command| match command {
                Command::Input { text, .. } => vec![text.clone()],
                Command::SendAll { lines, .. } => lines.clone(),
                _ => Vec::new(),
            })
            .collect()
    })
}

/// A server that batches and echoes: the lines of the tests' sends fit.
pub(super) const BATCHING: &str =
    "batch echo-message message-tags server-time draft/multiline=max-bytes=4096,max-lines=100";

/// [`rehearse`] on a welcomed client whose write succeeds.
pub(super) fn answer(
    view: &Entity<Conversation>,
    caps: &str,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    rehearse(view, Rehearsed::Welcomed(caps), cx)
}

#[gpui_kit::test]
fn an_eleventh_file_is_refused_with_a_notice_and_the_first_ten_stay(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let names: Vec<String> = (0..12).map(|n| format!("note{n}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let _folder = attach(&view, &names, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), MAX_ITEMS);
        assert_eq!(view.tray.notice, Some(LIMIT_NOTICE));
    });
    let _more = attach(&view, &["extra.txt"], cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), MAX_ITEMS);
        assert_eq!(view.tray.notice, Some(LIMIT_NOTICE));
    });
}

#[gpui_kit::test]
fn one_failed_upload_sends_nothing_and_keeps_the_draft(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let (lines, _api) = send(&view, &input, (&["good.txt", "fail.txt"], "with files"), cx);
    assert!(lines.is_empty());
    assert_eq!(text(&input, cx), "with files");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), 2);
        assert!(view.tray.idle());
    });
}

#[gpui_kit::test]
fn text_and_references_share_one_line_and_clear_once_echoed(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let (lines, _api) = send(&view, &input, (&["a.txt", "b.md"], "look"), cx);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("look UPL-"));
    let keys = subspace_api_client::chat_uploads::references(&lines[0]);
    assert_eq!(keys.len(), 2);
    assert!(lines[0].contains(" a.txt (") && lines[0].contains(" b.md ("));
    assert_eq!(answer(&view, BATCHING, cx).len(), 1, "one PRIVMSG");
    assert!(
        !view.read_with(cx, |view, _| view.tray.is_empty()),
        "until the echo"
    );
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    let room = view.read_with(cx, |view, _| view.buffer.clone());
    crate::tests::inject(
        &hub,
        vec![Event::Line(crate::fixtures::message(
            &room, "subspace", &lines[0], 5,
        ))],
        cx,
    );
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.tray.is_empty()));
    assert_eq!(text(&input, cx), "");
}

#[gpui_kit::test]
fn references_that_do_not_fit_follow_the_text_in_whole_packed_lines(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let long: Vec<String> = (0..6)
        .map(|n| format!("{n}{}.txt", "n".repeat(110)))
        .collect();
    let names: Vec<&str> = long.iter().map(String::as_str).collect();
    let _folder = attach(&view, &names, cx);
    submit(&view, "", cx);
    let lines = sent(&view, cx);
    assert!(lines.len() > 1, "{lines:?}");
    let budget = view.read_with(cx, |view, cx| {
        crate::hub::message_budget(view.hub.read(cx), &view.buffer)
    });
    let mut keys = Vec::new();
    for line in &lines {
        assert!(line.len() <= budget);
        keys.extend(subspace_api_client::chat_uploads::references(line));
    }
    assert_eq!(keys.len(), 6, "no reference is split");
}

#[gpui_kit::test]
fn commands_cannot_carry_attachments(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["a.txt"], cx);
    submit(&view, "/me waves", cx);
    assert!(sent(&view, cx).is_empty());
    assert_eq!(view.read_with(cx, |view, _| view.tray.items.len()), 1);
}
