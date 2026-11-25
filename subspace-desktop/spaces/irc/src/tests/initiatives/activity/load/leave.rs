//! Closing the view or leaving Channels: no page is asked for after,
//! and the officer's turns and their watch close.

use std::io::{BufRead as _, BufReader, Write as _};
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};

use super::super::super::super::remove::settle;
use super::{Gateway, Irc, Secret, TcpListener, drawn, json, label, on, page, paged, served, tool};

#[gpui_kit::test]
fn closing_the_view_before_a_read_asks_for_no_page_and_shows_nothing(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "short");
    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        window.click("irc-activity-back", cx);
    });
    settle(cx);
    assert!(served.asked().is_empty(), "{:?}", served.asked());
    board.read_with(cx, |board, _| assert!(board.inspector_activity.is_none()));
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    assert_eq!(drawn(&on("out:0"), cx), "");
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
                    page(&gateway, &target).unwrap_or_default()
                } else {
                    let bytes = gateway.item.lock().unwrap().1.len();
                    json!({
                        "agent": {"id": "aurora-lyra", "role": "captain",
                                  "backend": "opencode", "collected": true},
                        "turns": [super::super::turn("t-now", "running", 0, paged(6, bytes))],
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

/// The event streams the gateway was asked for, in order.
fn streams(served: &Gateway) -> Vec<String> {
    let mut asked = served.asked();
    asked.retain(|target| target.starts_with("/api/v1/events"));
    asked
}

/// `#aurora-lyra` open with the fleet's service on `served`, signed in,
/// followed by another holder as the fleet's other views would be, and
/// the ship as live as the fixture's whatever the loopback stream says
/// of the fleet (this share is the last the hub reads).
fn followed_ship<'a>(
    served: &Arc<Gateway>,
    cx: &'a mut TestAppContext,
) -> (
    Entity<Irc>,
    Entity<subspace_client_fleet::FleetApi>,
    subspace_client_fleet::FleetFollow,
    &'a mut VisualTestContext,
) {
    use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};

    let record = FleetRecord::new("aurora", "Aurora", Some(fleet_gateway(served)));
    let service = cx.update(|cx| FleetApi::install_new(TokenStore::memory(), vec![record], cx));
    cx.run_until_parked();
    let (board, hub, cx) = super::super::super::fleet::ship(cx);
    let saved = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new("fixture-throwaway"), cx)
    });
    settle(cx);
    assert!(cx.foreground_executor().block_test(saved).is_ok());
    let follow = service.update(cx, |service, cx| service.follow("aurora", cx));
    hub.update(cx, |hub, cx| {
        let lives = [("aurora".to_owned(), crate::live::tests::live())].into();
        crate::live::watch::share(hub, u64::MAX, lives, cx);
    });
    settle(cx);
    (board, service, follow, cx)
}

/// GPT review P2 at `9c53ac21`: leaving Channels closes the turns. A read
/// under way asks for no page, and the one event stream drops `?agent=`,
/// while another holder's follow of the fleet keeps the stream open.
#[gpui_kit::test]
fn leaving_channels_closes_the_turns_their_reads_and_their_watch(cx: &mut TestAppContext) {
    let served = Arc::new(Gateway::default());
    served.serve(6, tool(6, &"x".repeat(100_000)));
    let (board, service, _follow, cx) = followed_ship(&served, cx);
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    let watched = "/api/v1/events?agent=aurora-lyra";
    assert!(
        until(cx, |_| served.asked().iter().any(|t| t == watched)),
        "{:?}",
        served.asked()
    );
    assert!(
        until(cx, |cx| label(&on("load"), cx).is_some()),
        "{:?}",
        served.asked()
    );

    // Load, and leave Channels before the read asks for its first page.
    let pages_before = |served: &Gateway| {
        let asked = served.asked();
        asked.iter().filter(|t| t.contains("page=")).count()
    };
    assert_eq!(pages_before(&served), 0);
    let before = streams(&served).len();
    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        board.update(cx, |board, cx| {
            subspace_framework::dashboard::Dashboard::deactivate(board, window, cx);
        });
    });
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.inspector_activity.is_none()));
    // The stream opens again without the officer, and stays followed.
    assert!(
        until(cx, |_| streams(&served)[before..]
            .iter()
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
