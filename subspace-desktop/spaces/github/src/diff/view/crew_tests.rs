use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, point, px, size};
use serde_json::json;
use subspace_client_fleet::{
    FleetApi, FleetRecord, TokenStore, diff::DiffMode, navigation::OpenCrewDiff,
};
use subspace_framework::client_test_support::Mock;
use subspace_framework::core::Secret;
use subspace_framework::dashboard::prefs;

use super::super::model::{Layout, VIEWED_KEY};
use super::{DiffSource, DiffView, OpenDiff, source::FleetSource};

const FLEET: &str = "example-fleet";
const SHIP: &str = "example-ship";
const CREW: &str = "example-crew";

mod reads;
mod refresh;
mod retry;
pub(super) use refresh::ThreadedReads;
pub(super) use retry::ReadGate;

fn fixture(cursor: bool) -> String {
    let text = "wide".repeat(2000);
    json!({ "ship": SHIP, "member": CREW, "mode": "branch", "fingerprint": "B", "computedAt": 1,
        "total": if cursor { 2 } else { 1 }, "additions": 1, "deletions": 0,
        "nextCursor": cursor.then_some("page-list"), "files": [{ "path": "example.txt", "status": "modified",
        "additions": 1, "deletions": 0, "patch": format!("@@ -0,0 +1 @@\n+{text}END-OF-LINE\n"), "patchNextPage": "page-file" }]
    }).to_string()
}

pub(super) fn mount<'a>(
    cx: &'a mut TestAppContext,
    mock: &Mock,
) -> (Entity<DiffView>, &'a mut VisualTestContext) {
    let api = cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
        FleetApi::install_new(
            TokenStore::memory(),
            vec![FleetRecord::new(
                FLEET,
                "Example fleet",
                Some(mock.base_url().into()),
            )],
            cx,
        )
    });
    let saved = api.update(cx, |api, cx| {
        api.save_token(FLEET, Secret::new("fixture-token"), cx)
    });
    assert!(cx.foreground_executor.block_test(saved).is_ok());
    let (view, window) = cx.add_window_view(|window, cx| {
        DiffView::new(
            OpenDiff {
                source: DiffSource::Fleet(FleetSource {
                    target: OpenCrewDiff {
                        fleet: FLEET.into(),
                        ship: SHIP.into(),
                        member: CREW.into(),
                        project: None,
                    },
                }),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        )
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    (view, window)
}

#[gpui_kit::test]
fn a_cursor_conflict_or_missing_page_offers_refresh_not_connection_help(cx: &mut TestAppContext) {
    for status in [409, 404] {
        let mock = Mock::new(move |request| {
            if request.path.contains("cursor=") {
                (status, String::new())
            } else if request.path.contains("/diff?") {
                (200, fixture(true))
            } else {
                (404, String::new())
            }
        });
        let (view, window) = mount(cx, &mock);
        view.read_with(window, |view, _| {
            assert!(view.fleet.changed());
            assert_eq!(
                view.error.as_deref(),
                Some("The checkout changed while loading. Refresh to try again.")
            );
        });
        window.update(|window, _| assert!(window.find("crew-diff-error-refresh").visible()));
        window.update(|window, cx| window.click("crew-diff-error-refresh", cx));
        window.run_until_parked();
        assert_eq!(
            mock.requests()
                .iter()
                .filter(|request| request.path.contains("cursor="))
                .count(),
            2
        );
    }
}

#[gpui_kit::test]
fn a_feed_catching_up_to_the_http_snapshot_keeps_paging_available(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("/diff/file?") {
            assert!(request.path.contains("&at=B&page=page-file"));
            return (
                200,
                json!({"fingerprint": "B", "unstable": true,
                "file": {"path": "example.txt", "status": "modified", "patch": "+SECOND-PAGE\n"}})
                .to_string(),
            );
        }
        if request.path.contains("/diff?") {
            (200, fixture(false))
        } else {
            (404, String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    view.update(window, |view, cx| {
        // The feed's baseline observed when the HTTP snapshot was applied.
        view.fleet.live_fingerprint = Some("A".to_owned());
        view.observe_crew_fingerprint(Some("A".to_owned()));
        assert!(!view.fleet.changed());
        view.observe_crew_fingerprint(Some("B".to_owned()));
        assert!(!view.fleet.changed());
        view.load_more(0, cx);
        assert!(view.fleet.loading.contains(&0));
    });
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert!(!view.fleet.changed());
        assert_eq!(view.fleet.banner(), Some("Changed while loading"));
        assert!(view.fleet.loading.is_empty());
        assert!(view.files[0].next_page.is_none());
        assert!(
            view.files[0]
                .file
                .patch
                .as_deref()
                .unwrap()
                .contains("SECOND-PAGE")
        );
    });
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.update(|window, _| {
        assert!(window.find("crew-diff-banner-refresh").visible());
    });
}

#[gpui_kit::test]
fn a_new_ship_event_shows_refresh_and_blocks_the_old_file_cursor(cx: &mut TestAppContext) {
    let send = Arc::new(AtomicBool::new(false));
    let go = send.clone();
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff?") {
            return (200, vec![], fixture(false));
        }
        if request.path.ends_with("/events") {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !go.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            return (
                200,
                vec![("Content-Type".into(), "text/event-stream".into())],
                format!(
                    "event: ship\ndata: {{\"id\":\"{SHIP}\",\"crewMembers\":[{{\"id\":\"{CREW}\",\"contexts\":[{{\"project\":null,\"git\":{{\"fingerprint\":\"C\"}}}}]}}]}}\n\n"
                ),
            );
        }
        (404, vec![], String::new())
    });
    let (view, window) = mount(cx, &mock);
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"))
    });
    send.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !view.read_with(window, |view, _| view.fleet.changed()) && Instant::now() < deadline {
        window.executor().advance_clock(Duration::from_millis(250));
        window.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
    view.update(window, |view, cx| {
        assert_eq!(view.fleet.banner(), Some("Changed since loaded"));
        view.load_more(0, cx);
        assert!(view.fleet.loading.is_empty());
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.update(|window, _| assert!(window.find("crew-diff-banner-refresh").visible()));
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("/diff/file?"))
    );
}

#[gpui_kit::test]
fn mode_changes_discard_old_file_pages_and_crew_marks_do_not_touch_prefs(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("/diff?") {
            (200, fixture(false))
        } else {
            (404, String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    view.update(window, |view, cx| {
        prefs::set(cx, VIEWED_KEY, r#"{"example": ["example.txt"]}"#.to_owned());
        let before = prefs::get(cx, VIEWED_KEY);
        view.toggle_viewed_file(0, true, cx);
        assert_eq!(prefs::get(cx, VIEWED_KEY), before);
        assert!(view.viewed.is_empty());
        assert!(!view.collapsed.contains(&0));
        let previous = view.fleet.generation;
        view.fleet.loading.insert(0);
        view.set_mode(DiffMode::Working, None, cx);
        assert!(view.fleet.loading.is_empty());
        view.finish_page(
            previous,
            0,
            Err(subspace_client_fleet::Error::StaleDiff),
            cx,
        );
        assert!(
            !view.fleet.changed(),
            "a late conflict cannot invalidate the new mode"
        );
    });
}

#[gpui_kit::test]
fn the_end_of_an_unbroken_changed_line_can_be_scrolled_into_view_in_both_layouts(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|request| {
        if request.path.contains("/diff?") {
            (200, fixture(false))
        } else {
            (404, String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    window.update(|window, _| window.resize(size(px(560.), px(400.))));
    for (layout, id) in [
        (Layout::Unified, "diff-line-left"),
        (Layout::Split, "diff-line-right"),
    ] {
        view.update(window, |view, cx| view.set_layout(layout, cx));
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.update(|window, cx| window.click((id, 0usize), cx));
        window.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(window, |view, _| {
            let line = view
                .full_line
                .as_ref()
                .expect("full-line button opens the reader");
            let (text, scroll) = line.parts();
            assert!(text.ends_with("END-OF-LINE"));
            let end = scroll.max_offset().x;
            assert!(end > px(0.), "the entire unbroken line has a scroll extent");
            scroll.set_offset(point(-end, px(0.)));
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(window, |view, _| {
            let (_, scroll) = view.full_line.as_ref().unwrap().parts();
            assert_eq!(-scroll.offset().x, scroll.max_offset().x);
        });
        window.update(|window, cx| window.click("diff-full-line-close", cx));
    }
}
