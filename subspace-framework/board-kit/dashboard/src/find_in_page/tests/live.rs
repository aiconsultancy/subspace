//! Content refresh is not a new user navigation request.
use super::{Content, press};
use crate::find_in_page::{Document, FindInPage, Provider};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct Live {
    pane: Entity<FindInPage>,
    docs: Rc<RefCell<Vec<Document>>>,
    reveals: Rc<Cell<usize>>,
}
fn setup<'a>(query: &str, cx: &'a mut TestAppContext) -> (Live, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let reveals = Rc::new(Cell::new(0));
    let docs = Rc::new(RefCell::new(vec![Document::markdown("first", "needle")]));
    let read = docs.clone();
    let count = reveals.clone();
    let provider = Provider {
        virtual_content: Rc::new(|_| true),
        snapshot: Rc::new(move |_| ("live".into(), read.borrow().clone())),
        reveal: Rc::new(move |_, _, _| count.set(count.get() + 1)),
    };
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| Content {
            focus: cx.focus_handle(),
        });
        content.read(cx).focus.clone().focus(window, cx);
        FindInPage::new(content.into(), provider, window, cx)
    });
    cx.run_until_parked();
    press(crate::secondary!("f"), cx);
    cx.update(|window, cx| window.input(query, cx));
    cx.run_until_parked();
    (
        Live {
            pane,
            docs,
            reveals,
        },
        cx,
    )
}
fn add(fixture: &Live, source: &str, cx: &mut VisualTestContext) {
    fixture
        .docs
        .borrow_mut()
        .push(Document::markdown("second", source));
    fixture.pane.update(cx, |pane, cx| {
        pane.session.update(cx, |s, cx| s.refresh_provider(cx))
    });
    cx.run_until_parked();
}
#[gpui_kit::test]
fn incoming_content_updates_count_without_revealing_the_previous_match_again(
    cx: &mut TestAppContext,
) {
    let (fixture, cx) = setup("needle", cx);
    let navigations = fixture.reveals.get();
    assert!(navigations > 0);
    add(&fixture, "needle later", cx);
    assert_eq!(fixture.reveals.get(), navigations);
    fixture.pane.read_with(cx, |pane, cx| {
        assert_eq!(pane.session.read(cx).cursor.position(), (1, 2))
    });
    press("enter", cx);
    assert_eq!(fixture.reveals.get(), navigations + 1);
}
#[gpui_kit::test]
fn the_first_live_match_after_zero_results_does_not_replay_an_old_query(cx: &mut TestAppContext) {
    let (fixture, cx) = setup("future", cx);
    fixture.pane.read_with(cx, |pane, cx| {
        assert_eq!(pane.session.read(cx).cursor.position(), (0, 0))
    });
    assert_eq!(fixture.reveals.get(), 0);
    add(&fixture, "future message", cx);
    fixture.pane.read_with(cx, |pane, cx| {
        assert_eq!(pane.session.read(cx).cursor.position(), (1, 1))
    });
    assert_eq!(fixture.reveals.get(), 0);
    press("enter", cx);
    assert_eq!(fixture.reveals.get(), 1);
}

#[gpui_kit::test]
fn opening_an_open_find_only_refocuses_without_resetting_the_match(cx: &mut TestAppContext) {
    let (fixture, cx) = setup("needle", cx);
    add(&fixture, "needle later", cx);
    press("enter", cx);
    let before = fixture.reveals.get();
    press(crate::secondary!("f"), cx);
    fixture.pane.read_with(cx, |p, cx| {
        assert_eq!(p.session.read(cx).cursor.position(), (2, 2))
    });
    assert_eq!(fixture.reveals.get(), before);
}
#[gpui_kit::test]
fn stepping_only_notifies_native_documents_whose_highlights_changed(cx: &mut TestAppContext) {
    use gpui_kit::ElementId;
    use std::collections::HashSet;
    let (fixture, cx) = setup("needle", cx);
    fixture.docs.borrow_mut().extend([
        Document::markdown("second", "needle later"),
        Document::markdown("third", "unrelated text"),
    ]);
    fixture.pane.update(cx, |p, cx| {
        p.session.update(cx, |s, cx| s.refresh_provider(cx))
    });
    cx.run_until_parked();
    let states = fixture.pane.read_with(cx, |p, cx| {
        ["first", "second", "third"].map(|id| {
            p.session
                .read(cx)
                .pooled(&ElementId::from(id), None)
                .unwrap()
        })
    });
    let changed = Rc::new(RefCell::new(HashSet::new()));
    let _subscriptions: Vec<_> = states
        .iter()
        .map(|state| {
            let changed = changed.clone();
            let id = state.entity_id();
            cx.update(|_, cx| {
                cx.observe(state, move |_, _| {
                    changed.borrow_mut().insert(id);
                })
            })
        })
        .collect();
    press("enter", cx);
    assert_eq!(changed.borrow().len(), 2);
    assert!(!changed.borrow().contains(&states[2].entity_id()));
}
