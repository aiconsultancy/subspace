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

/// The ID of the paged card's element `part`.
fn on(part: &str) -> String {
    format!("{CARD}:{part}")
}

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
    cx.update(|window, cx| window.click(SharedString::from(on("load")), cx));
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
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load in full"));
    load(cx);
    assert_eq!(served.asked().len(), 2, "both pages: {:?}", served.asked());
    assert!(drawn(&on("out:0"), cx).ends_with("FIRST-END"));
    assert!(
        drawn(&on("raw"), cx).contains("RAW-PAGED-SENTINEL"),
        "the input only the stream carried shows in a paged item too"
    );
    assert_eq!(label(&on("load"), cx), None, "read in full");

    // The item grew: the snapshot now names a newer version.
    let newer = format!("{}SECOND-END", "y".repeat(100_000));
    served.serve(7, tool(7, &newer));
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(7, bytes))), cx);
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load the latest"));
    assert!(
        drawn(&on("out:0"), cx).ends_with("FIRST-END"),
        "the older version shows meanwhile"
    );
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("SECOND-END"));
    assert_eq!(label(&on("load"), cx), None);
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
    let id = on("out:0");
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

mod budget;
mod kinds;
mod leave;
