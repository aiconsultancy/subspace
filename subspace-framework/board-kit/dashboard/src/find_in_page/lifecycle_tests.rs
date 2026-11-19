use super::Session;
use crate::find_in_page::{Document, Provider};
use gpui_kit::base::TextViewState;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use std::{cell::Cell, rc::Rc};
fn provider(documents: Vec<Document>) -> Provider {
    Provider {
        virtual_content: Rc::new(|_| true),
        snapshot: Rc::new(move |_| (SharedString::from("room"), documents.clone())),
        ..Default::default()
    }
}
#[gpui_kit::test]
fn virtual_documents_use_the_same_parser_guard_as_visible_rich_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = "1. ```\n- x\n";
    let guarded = crate::rich::markdown_guard::prepare(raw.to_owned().into());
    let expected = cx.update(|cx| cx.new(|cx| TextViewState::markdown(&guarded, cx)));
    let session =
        cx.update(|cx| cx.new(|_| Session::new(provider(vec![Document::markdown("fence", raw)]))));
    session.update(cx, |s, cx| s.show(cx));
    cx.run_until_parked();
    session.read_with(cx, |s, cx| {
        let actual = s.pooled(&ElementId::from("fence"), None).unwrap();
        assert_eq!(
            actual.read(cx).rendered_text().as_str(),
            expected.read(cx).rendered_text().as_str()
        );
        assert!(actual.read(cx).rendered_text().as_str().contains("```"));
    });
}
#[gpui_kit::test]
fn opening_and_reopening_find_reuses_the_selected_live_native_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("alpha bravo charlie", cx)));
    state.update(cx, |state, cx| state.select_all(cx));
    let before = state.read_with(cx, |state, _| state.selected_text());
    assert!(!before.is_empty());
    let session = cx.update(|cx| {
        cx.new(|_| {
            Session::new(provider(vec![Document::markdown(
                "live",
                "alpha bravo charlie",
            )]))
        })
    });
    session.update(cx, |s, cx| {
        s.rich(&"live".into(), &state, Some("room".into()), cx)
    });
    for _ in 0..2 {
        session.update(cx, |s, cx| s.show(cx));
        cx.run_until_parked();
        session.read_with(cx, |s, cx| {
            assert_eq!(s.pooled(&"live".into(), None), Some(state.clone()));
            assert_eq!(state.read(cx).selected_text(), before);
        });
        session.update(cx, |s, cx| s.close(cx));
        cx.run_until_parked();
    }
}
#[gpui_kit::test]
fn registrations_are_released_even_when_find_has_never_been_opened(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let session = cx.update(|cx| cx.new(|_| Session::new(Provider::default())));
    for index in 0..300 {
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("transient row", cx)));
        session.update(cx, |s, cx| {
            s.rich(
                &ElementId::from(SharedString::from(format!("row-{index}"))),
                &state,
                None,
                cx,
            )
        });
    }
    cx.run_until_parked();
    // Native release effects are flushed by an app update, not executor parking.
    cx.update(|_| {});
    session.read_with(cx, |s, _| assert_eq!(s.entries.len(), 0));
}
#[gpui_kit::test]
fn recreated_native_text_reapplies_highlights_without_a_query_edit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let session = cx.update(|cx| cx.new(|_| Session::new(Provider::default())));
    let make = |cx: &mut TestAppContext| {
        cx.update(|cx| cx.new(|cx| TextViewState::markdown("needle", cx)))
    };
    let original = make(cx);
    session.update(cx, |s, cx| {
        s.rich(&"same".into(), &original, None, cx);
        s.show(cx);
        s.query("needle".into(), cx);
    });
    cx.run_until_parked();
    let replacement = make(cx);
    cx.run_until_parked();
    let notified = Rc::new(Cell::new(0));
    let count = notified.clone();
    let _observation =
        cx.update(|cx| cx.observe(&replacement, move |_, _| count.set(count.get() + 1)));
    session.update(cx, |s, cx| s.rich(&"same".into(), &replacement, None, cx));
    cx.run_until_parked();
    assert!(
        notified.get() > 0,
        "highlights must be applied to the replacement state"
    );
    session.read_with(cx, |s, _| {
        assert_eq!(s.cursor.position(), (1, 1));
        assert_eq!(s.snapshot[0].1, replacement.entity_id());
    });
}
#[gpui_kit::test]
fn match_order_uses_structural_reading_order_after_remounting(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let session = cx.update(|cx| cx.new(|_| Session::new(Provider::default())));
    let bottom = cx.update(|cx| cx.new(|cx| TextViewState::markdown("needle bottom", cx)));
    let top = cx.update(|cx| cx.new(|cx| TextViewState::markdown("needle top", cx)));
    session.update(cx, |s, cx| {
        s.rich(&"bottom".into(), &bottom, None, cx);
        s.rich(&"top".into(), &top, None, cx);
        assert!(s.order_changed(&"bottom".into(), None, 1));
        assert!(!s.order_changed(&"missing".into(), None, 0));
        s.order(&"bottom".into(), None, 1, cx);
        assert!(!s.order_changed(&"bottom".into(), None, 1));
        s.order(&"top".into(), None, 0, cx);
        s.show(cx);
        s.query("needle".into(), cx);
    });
    cx.run_until_parked();
    let replacement = cx.update(|cx| cx.new(|cx| TextViewState::markdown("needle top", cx)));
    session.update(cx, |s, cx| s.rich(&"top".into(), &replacement, None, cx));
    cx.run_until_parked();
    session.read_with(cx, |s, _| {
        assert_eq!(
            s.cursor.active().unwrap().document.as_ref(),
            super::key(&"top".into()).as_ref()
        );
        assert_eq!(s.cursor.position(), (1, 2));
    });
}
#[gpui_kit::test]
fn a_large_loaded_transcript_groups_hits_once_and_reuses_pool_states(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let docs = (0..5_000)
        .map(|i| {
            Document::markdown(
                SharedString::from(format!("loaded-{i}")),
                "alpha alpha alpha alpha alpha alpha alpha alpha alpha alpha",
            )
        })
        .collect();
    let session = cx.update(|cx| cx.new(|_| Session::new(provider(docs))));
    let start = std::time::Instant::now();
    session.update(cx, |s, cx| s.show(cx));
    let synchronous_open = start.elapsed();
    cx.run_until_parked();
    let opened = start.elapsed();
    let start = std::time::Instant::now();
    session.update(cx, |s, cx| s.query("a".into(), cx));
    cx.run_until_parked();
    let searched = start.elapsed();
    session.read_with(cx, |s, cx| {
        assert_eq!(s.cursor.hits().len(), 100_000);
        assert_eq!(s.ranges.len(), 5_000);
        let id = ElementId::from("loaded-0");
        assert_eq!(s.highlights(&id, None, cx).len(), 20);
    });
    let first = session.read_with(cx, |s, _| s.pooled(&"loaded-0".into(), None).unwrap());
    let start = std::time::Instant::now();
    session.update(cx, |s, cx| s.refresh_provider(cx));
    cx.run_until_parked();
    let refreshed = start.elapsed();
    session.read_with(cx, |s, _| {
        assert_eq!(s.pooled(&"loaded-0".into(), None), Some(first.clone()))
    });
    eprintln!(
        "5000 rows / 100000 hits: open sync={synchronous_open:?}, settled={opened:?}, query={searched:?}, unchanged refresh={refreshed:?}"
    );
}

#[path = "tests/plain_cache.rs"]
mod plain_cache;
