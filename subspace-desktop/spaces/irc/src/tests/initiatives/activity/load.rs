//! An item too large to send inline: **Load in full** reads it page by
//! page and shows it whole, of a kind this version knows or as its JSON, a
//! newer version offers **Load the latest**, a long text shows in parts
//! whose last bytes are in reach, an item larger than the view can hold
//! shows one page at a time within its budget, the item goes with its
//! turn, a late reply is dropped, and closing the view before the read
//! starts asks for no page and shows nothing.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_client_fleet::{Error, FleetAddress, FleetClient};
use subspace_framework::core::Secret;

use super::super::super::remove::settle;
use super::{answer, drawn, opened, refresh, running};
use crate::Irc;
use crate::view::inspector::activity::{
    held_bytes, late_reply, loaded_keys, read_of, set_budget, set_client,
};

/// The most bytes of item JSON one page carries.
const PAGE: usize = 64 * 1024;
/// The card of the paged item.
const CARD: &str = "irc-activity:t-now:i3";

/// What the loopback gateway serves of item `i3`, and every request it
/// answered.
#[derive(Default)]
struct Gateway {
    /// The item's `seq` and complete JSON.
    item: Mutex<(u64, String)>,
    asked: Mutex<Vec<String>>,
}

impl Gateway {
    fn serve(&self, seq: u64, item: Value) {
        *self.item.lock().unwrap() = (seq, item.to_string());
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// A loopback gateway answering each page of `i3` from `gateway`, and a
/// client for it.
fn gateway(gateway: &Arc<Gateway>) -> FleetClient {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let gateway = Arc::clone(gateway);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let target = first.split(' ').nth(1).unwrap_or_default().to_owned();
            gateway.asked.lock().unwrap().push(target.clone());
            let body = page(&gateway, &target).to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let address = FleetAddress::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    FleetClient::new(address, Secret::new("fixture-throwaway"))
}

/// The page `target` asks for. The fixture's JSON is ASCII, so any byte
/// is a character boundary.
fn page(gateway: &Gateway, target: &str) -> Value {
    let number: usize = target
        .split("page=")
        .nth(1)
        .and_then(|rest| rest.split('&').next()?.parse().ok())
        .unwrap_or(0);
    let (seq, item) = gateway.item.lock().unwrap().clone();
    let pages: Vec<&str> = item
        .as_bytes()
        .chunks(PAGE)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect();
    json!({
        "agent": "aurora-lyra", "turn": "t-now", "item": "i3", "seq": seq,
        "page": number, "pages": pages.len(), "bytes": item.len(),
        "data": pages.get(number).copied().unwrap_or_default()
    })
}

/// Item `i3` in full: a tool call whose input only the stream carried and
/// whose output is `output`.
fn tool(seq: u64, output: &str) -> Value {
    json!({"id": "i3", "seq": seq, "startedAt": 1, "updatedAt": 2, "kind": "tool",
           "callId": "call-2", "name": "read", "status": "completed",
           "rawInput": "{\"path\": \"RAW-PAGED-SENTINEL\"", "input": null,
           "executed": true, "metadata": null,
           "content": [{"type": "text", "text": output}], "error": null})
}

/// The snapshot's word for `i3` at `seq`: too large to send inline.
fn paged(seq: u64, bytes: usize) -> Value {
    let pages = bytes.div_ceil(PAGE);
    json!([{"id": "i3", "seq": seq, "startedAt": 1, "updatedAt": 2, "kind": "tool",
            "status": "completed", "callId": "call-2", "name": "read",
            "paged": {"pages": pages, "bytes": bytes}}])
}

/// The turn view with `i3` at `seq`, read through `gateway`, which serves
/// it with `output`.
fn served<'a>(
    cx: &'a mut TestAppContext,
    seq: u64,
    output: &str,
) -> (Entity<Irc>, Arc<Gateway>, &'a mut VisualTestContext) {
    let (board, cx) = opened(cx);
    let served = Arc::new(Gateway::default());
    served.serve(seq, tool(seq, output));
    let client = gateway(&served);
    board.update(cx, |board, _| {
        set_client(board.inspector_activity.as_mut().unwrap(), client);
    });
    let bytes = served.item.lock().unwrap().1.len();
    answer(&board, Ok(running(paged(seq, bytes))), cx);
    (board, served, cx)
}

fn load(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(SharedString::from(format!("{CARD}:load")), cx));
    settle(cx);
}

fn label(id: &str, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| {
        window
            .try_find(SharedString::from(id.to_owned()))
            .and_then(|found| found.label().map(str::to_owned))
    })
}

#[gpui_kit::test]
fn load_in_full_shows_the_whole_item_and_offers_the_latest_once_it_changes(
    cx: &mut TestAppContext,
) {
    let output = format!("{}FIRST-END", "x".repeat(100_000));
    let (board, served, cx) = served(cx, 6, &output);
    assert_eq!(
        label(&format!("{CARD}:load"), cx).as_deref(),
        Some("Load in full")
    );
    load(cx);
    assert_eq!(served.asked().len(), 2, "both pages: {:?}", served.asked());
    assert!(drawn(&format!("{CARD}:out:0"), cx).ends_with("FIRST-END"));
    assert!(
        drawn(&format!("{CARD}:raw"), cx).contains("RAW-PAGED-SENTINEL"),
        "the input only the stream carried shows in a paged item too"
    );
    assert_eq!(label(&format!("{CARD}:load"), cx), None, "read in full");

    // The item grew: the snapshot now names a newer version.
    let newer = format!("{}SECOND-END", "y".repeat(100_000));
    served.serve(7, tool(7, &newer));
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(7, bytes))), cx);
    assert_eq!(
        label(&format!("{CARD}:load"), cx).as_deref(),
        Some("Load the latest")
    );
    assert!(
        drawn(&format!("{CARD}:out:0"), cx).ends_with("FIRST-END"),
        "the older version shows meanwhile"
    );
    load(cx);
    assert!(drawn(&format!("{CARD}:out:0"), cx).ends_with("SECOND-END"));
    assert_eq!(label(&format!("{CARD}:load"), cx), None);
}

#[gpui_kit::test]
fn a_long_output_shows_in_parts_whose_last_reaches_its_end(cx: &mut TestAppContext) {
    let mut output: String = (0..20_000)
        .map(|line| format!("line {line:05} of the log\n"))
        .collect();
    output.push_str("LAST-BYTES");
    assert!(output.len() > 256 * 1024);
    let (_board, _served, cx) = served(cx, 6, &output);
    load(cx);
    let id = format!("{CARD}:out:0");
    let parts = output.len().div_ceil(PAGE);
    assert_eq!(
        drawn(&format!("{id}:part"), cx),
        format!("Part 1 of {parts}")
    );
    assert!(drawn(&id, cx).starts_with("line 00000 of the log"));
    cx.update(|window, cx| window.click(SharedString::from(format!("{id}:part-last")), cx));
    settle(cx);
    assert_eq!(
        drawn(&format!("{id}:part"), cx),
        format!("Part {parts} of {parts}")
    );
    assert!(drawn(&id, cx).ends_with("LAST-BYTES"), "the end in reach");
    cx.update(|window, cx| window.click(SharedString::from(format!("{id}:part-previous")), cx));
    settle(cx);
    assert_eq!(
        drawn(&format!("{id}:part"), cx),
        format!("Part {} of {parts}", parts - 1)
    );
    cx.update(|window, cx| window.click(SharedString::from(format!("{id}:part-first")), cx));
    settle(cx);
    assert_eq!(
        drawn(&format!("{id}:part"), cx),
        format!("Part 1 of {parts}")
    );
}

#[gpui_kit::test]
fn an_item_goes_with_its_turn(cx: &mut TestAppContext) {
    let (board, _served, cx) = served(cx, 6, "short");
    load(cx);
    let loaded = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            loaded_keys(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert_eq!(loaded(&board, cx), [("t-now".to_owned(), "i3".to_owned())]);
    // The snapshot no longer keeps the turn.
    let mut gone = running(json!([]));
    gone.turns.clear();
    refresh(&board, cx);
    answer(&board, Ok(gone), cx);
    assert!(loaded(&board, cx).is_empty(), "let go with its turn");
}

#[gpui_kit::test]
fn closing_the_view_before_a_read_asks_for_no_page_and_shows_nothing(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "short");
    cx.update(|window, cx| {
        window.click(SharedString::from(format!("{CARD}:load")), cx);
        window.click("irc-activity-back", cx);
    });
    settle(cx);
    assert!(served.asked().is_empty(), "{:?}", served.asked());
    board.read_with(cx, |board, _| assert!(board.inspector_activity.is_none()));
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    assert_eq!(drawn(&format!("{CARD}:out:0"), cx), "");
}

/// The view's turn view with `i3` served as `item` at `seq`.
fn served_as<'a>(
    cx: &'a mut TestAppContext,
    seq: u64,
    item: Value,
) -> (Entity<Irc>, Arc<Gateway>, &'a mut VisualTestContext) {
    let (board, served, cx) = served(cx, seq, "short");
    served.serve(seq, item);
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(seq, bytes))), cx);
    (board, served, cx)
}

/// GPT review P1 at `9c53ac21`: a paged item of a kind this version does
/// not know loads and shows as its JSON, down to its last bytes.
#[gpui_kit::test]
fn a_paged_item_of_an_unknown_kind_shows_its_json_to_the_end(cx: &mut TestAppContext) {
    let body = format!("{}UNKNOWN-KIND-END", "u".repeat(100_000));
    let item = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                      "kind": "patch", "body": body});
    let (_board, served, cx) = served_as(cx, 6, item);
    load(cx);
    assert_eq!(served.asked().len(), 2, "both pages: {:?}", served.asked());
    assert!(!drawn(&format!("{CARD}:unknown"), cx).is_empty());
    assert!(drawn(&format!("{CARD}:json"), cx).contains("UNKNOWN-KIND-END"));
    assert_eq!(label(&format!("{CARD}:load"), cx), None, "read in full");
}

/// GPT review P1 at `9c53ac21`: a paged item of a known kind in a shape
/// this version does not know loads and shows as its JSON too.
#[gpui_kit::test]
fn a_paged_item_of_a_changed_shape_shows_its_json_to_the_end(cx: &mut TestAppContext) {
    let tail = format!("{}CHANGED-SHAPE-END", "c".repeat(100_000));
    let item = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                      "kind": "tool", "status": 7, "tail": tail});
    let (_board, _served, cx) = served_as(cx, 6, item);
    load(cx);
    assert!(drawn(&format!("{CARD}:json"), cx).contains("CHANGED-SHAPE-END"));
    assert!(
        drawn(&format!("{CARD}:failed"), cx).is_empty(),
        "never a failure"
    );
}

/// SUB decision `a17870f4`: an item larger than the view can hold shows
/// one page of its JSON at a time, the first and last in reach, and the
/// view never holds more than its budget.
#[gpui_kit::test]
fn an_item_over_the_budget_shows_page_by_page_within_it(cx: &mut TestAppContext) {
    let output = "x".repeat(200_000);
    let (board, served, cx) = served(cx, 6, &output);
    let budget = 3 * PAGE as u64;
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), budget);
    });
    let held = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            held_bytes(board.inspector_activity.as_ref().unwrap())
        })
    };
    load(cx);
    let json = served.item.lock().unwrap().1.clone();
    let pages = json.len().div_ceil(PAGE);
    assert_eq!(
        served.asked().len(),
        1,
        "page 0 alone: {:?}",
        served.asked()
    );
    assert!(drawn(&format!("{CARD}:parts"), cx).starts_with("Too large to hold whole"));
    assert_eq!(
        drawn(&format!("{CARD}:page"), cx),
        format!("Part 1 of {pages}")
    );
    assert_eq!(drawn(&format!("{CARD}:json-part"), cx), json[..PAGE]);
    assert!(held(&board, cx) <= budget);

    cx.update(|window, cx| window.click(SharedString::from(format!("{CARD}:page-last")), cx));
    settle(cx);
    assert_eq!(
        drawn(&format!("{CARD}:page"), cx),
        format!("Part {pages} of {pages}")
    );
    assert_eq!(
        drawn(&format!("{CARD}:json-part"), cx),
        json[(pages - 1) * PAGE..],
        "the last bytes in reach"
    );
    assert!(
        served
            .asked()
            .last()
            .unwrap()
            .contains(&format!("page={}", pages - 1)),
        "{:?}",
        served.asked()
    );
    assert!(held(&board, cx) <= budget);

    cx.update(|window, cx| window.click(SharedString::from(format!("{CARD}:page-first")), cx));
    settle(cx);
    assert_eq!(
        drawn(&format!("{CARD}:page"), cx),
        format!("Part 1 of {pages}")
    );
    assert_eq!(drawn(&format!("{CARD}:json-part"), cx), json[..PAGE]);
    assert!(drawn(&format!("{CARD}:newer"), cx).is_empty());
    assert!(held(&board, cx) <= budget);
}

/// SUB decision `a17870f4`: each part is pinned to the version it began
/// with; once the item changed, it starts again from the first part of
/// the newest version and says so.
#[gpui_kit::test]
fn a_part_of_a_changed_item_starts_again_and_says_a_newer_version_is_there(
    cx: &mut TestAppContext,
) {
    let (board, served, cx) = served(cx, 6, &"x".repeat(200_000));
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), 3 * PAGE as u64);
    });
    load(cx);
    served.serve(7, tool(7, &"y".repeat(300_000)));
    let json = served.item.lock().unwrap().1.clone();
    let pages = json.len().div_ceil(PAGE);
    cx.update(|window, cx| window.click(SharedString::from(format!("{CARD}:page-next")), cx));
    settle(cx);
    assert!(drawn(&format!("{CARD}:newer"), cx).starts_with("A newer version is on the gateway"),);
    assert_eq!(
        drawn(&format!("{CARD}:page"), cx),
        format!("Part 1 of {pages}")
    );
    assert_eq!(drawn(&format!("{CARD}:json-part"), cx), json[..PAGE]);
    let asked = served.asked();
    assert!(asked[asked.len() - 2].contains("page=1") && asked[asked.len() - 1].contains("page=0"));
}

/// SUB decision `a17870f4`: a reply that arrives after its item was let
/// go, or after the item was read again, is dropped.
#[gpui_kit::test]
fn a_late_reply_is_dropped(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "FIRST-READ");
    load(cx);
    let first = board.read_with(cx, |board, _| {
        read_of(board.inspector_activity.as_ref().unwrap(), "t-now", "i3").unwrap()
    });
    // No room for any item: the next snapshot lets it go.
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), 0);
    });
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(6, bytes))), cx);
    let keys = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            loaded_keys(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert!(keys(&board, cx).is_empty(), "let go to make room");
    board.update(cx, |board, cx| {
        late_reply(board, ("t-now", "i3", first), Err(Error::NotFound), cx);
    });
    settle(cx);
    assert!(keys(&board, cx).is_empty(), "a reply after its item went");
    assert_eq!(
        label(&format!("{CARD}:load"), cx).as_deref(),
        Some("Load in full")
    );

    // Read again: the first read's reply is no answer to the second.
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), u64::MAX);
    });
    load(cx);
    assert!(drawn(&format!("{CARD}:out:0"), cx).ends_with("FIRST-READ"));
    board.update(cx, |board, cx| {
        late_reply(board, ("t-now", "i3", first), Err(Error::NotFound), cx);
    });
    settle(cx);
    assert!(drawn(&format!("{CARD}:failed"), cx).is_empty());
    assert!(drawn(&format!("{CARD}:out:0"), cx).ends_with("FIRST-READ"));
}

/// A loopback fleet gateway: its event stream stays open, the captain's
/// snapshot names `i3` as too large to send inline, and its pages are
/// served from `gateway`. Records each request.
fn fleet_gateway(gateway: &Arc<Gateway>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let gateway = Arc::clone(gateway);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let gateway = Arc::clone(&gateway);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let target = first.split(' ').nth(1).unwrap_or_default().to_owned();
                gateway.asked.lock().unwrap().push(target.clone());
                if target.starts_with("/api/v1/events") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                          event: heartbeat\ndata: {\"at\":1}\n\n",
                    );
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    return;
                }
                let body = if target.contains("page=") {
                    page(&gateway, &target)
                } else {
                    let bytes = gateway.item.lock().unwrap().1.len();
                    json!({
                        "agent": {"id": "aurora-lyra", "role": "captain",
                                  "backend": "opencode", "collected": true},
                        "turns": [super::turn("t-now", "running", 0, paged(6, bytes))],
                        "cursor": {"generation": "g1", "seq": 5}
                    })
                }
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// Runs the app's clock and the gateway's threads until `done`, for at
/// most about ten seconds.
fn until(cx: &mut VisualTestContext, mut done: impl FnMut(&mut VisualTestContext) -> bool) -> bool {
    for _ in 0..200 {
        if done(cx) {
            return true;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        settle(cx);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    done(cx)
}

/// GPT review P2 at `9c53ac21`: leaving Channels closes the turns. A read
/// under way asks for no page, and the one event stream drops `?agent=`,
/// while another holder's follow of the fleet keeps the stream open.
#[gpui_kit::test]
fn leaving_channels_closes_the_turns_their_reads_and_their_watch(cx: &mut TestAppContext) {
    use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};

    let served = Arc::new(Gateway::default());
    served.serve(6, tool(6, &"x".repeat(100_000)));
    let address = fleet_gateway(&served);
    let record = FleetRecord::new("aurora", "Aurora", Some(address));
    let service = cx.update(|cx| FleetApi::install_new(TokenStore::memory(), vec![record], cx));
    cx.run_until_parked();
    let (board, hub, cx) = super::super::fleet::ship(cx);
    let saved = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new("fixture-throwaway"), cx)
    });
    settle(cx);
    assert!(cx.foreground_executor().block_test(saved).is_ok());
    // Another holder, as the fleet's other views would be, follows it.
    let _follow = service.update(cx, |service, cx| service.follow("aurora", cx));
    super::super::live::set_fleet(&hub, super::super::live::live(), cx);
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    let watched = "/api/v1/events?agent=aurora-lyra";
    assert!(
        until(cx, |_| served.asked().iter().any(|t| t == watched)),
        "{:?}",
        served.asked()
    );
    assert!(
        until(cx, |cx| label(&format!("{CARD}:load"), cx).is_some()),
        "{:?}",
        served.asked()
    );

    // Load, and leave Channels before the read asks for its first page.
    let pages_before = |served: &Gateway| {
        served
            .asked()
            .iter()
            .filter(|t| t.contains("page="))
            .count()
    };
    assert_eq!(pages_before(&served), 0);
    let streams = served
        .asked()
        .iter()
        .filter(|t| t.starts_with("/api/v1/events"))
        .count();
    cx.update(|window, cx| {
        window.click(SharedString::from(format!("{CARD}:load")), cx);
        board.update(cx, |board, cx| {
            subspace_framework::dashboard::Dashboard::deactivate(board, window, cx);
        });
    });
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.inspector_activity.is_none()));
    // The stream opens again without the officer, and stays followed.
    assert!(
        until(cx, |_| served
            .asked()
            .iter()
            .filter(|t| t.starts_with("/api/v1/events"))
            .skip(streams)
            .any(|t| t == "/api/v1/events")),
        "{:?}",
        served.asked()
    );
    assert_eq!(
        pages_before(&served),
        0,
        "no page after leaving: {:?}",
        served.asked()
    );
    service.read_with(cx, |service, _| {
        assert!(
            matches!(
                service.state("aurora"),
                subspace_client_fleet::FleetState::Followed(_)
            ),
            "the other holder's follow remains"
        );
        assert!(service.activity_news("aurora", "aurora-lyra", 0).is_none());
    });
}
