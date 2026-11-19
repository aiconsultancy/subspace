use super::Session;
use crate::find_in_page::{Document, Provider};
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use std::{cell::RefCell, rc::Rc};
#[gpui_kit::test]
fn loaded_offscreen_rows_use_rendered_text_and_removed_retained_rows_leave_find(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let loaded = Rc::new(RefCell::new((
        SharedString::from("room-a"),
        vec![
            Document::markdown("virtual-a", "[label](https://example.org/needle)"),
            Document::markdown("virtual-b", "needle **NEEDLE**"),
        ],
    )));
    let source = loaded.clone();
    let session = cx.update(|cx| {
        cx.new(|_| {
            Session::new(Provider {
                virtual_content: Rc::new(|_| true),
                snapshot: Rc::new(move |_| source.borrow().clone()),
                reveal: Rc::new(|_, _, _| {}),
            })
        })
    });
    session.update(cx, |session, cx| session.show(cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| session.query("needle".into(), cx));
    let retained = session.read_with(cx, |session, cx| {
        assert_eq!(
            session.cursor.position(),
            (1, 2),
            "find text, not a hidden link destination"
        );
        let state = session.pooled(&ElementId::from("virtual-b"), None).unwrap();
        assert!(state.read(cx).rendered_text().as_str().contains("NEEDLE"));
        state
    });
    loaded.borrow_mut().1.clear();
    session.update(cx, |session, cx| session.refresh_provider(cx));
    cx.run_until_parked();
    session.read_with(cx, |session, cx| {
        assert_eq!(session.cursor.position(), (0, 0));
        assert!(
            !retained.read(cx).rendered_text().is_empty(),
            "the removed row is deliberately still retained"
        );
    });
    *loaded.borrow_mut() = (
        "room-b".into(),
        vec![Document::markdown("virtual-b", "unrelated text")],
    );
    session.update(cx, |session, cx| session.refresh_provider(cx));
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert_eq!(session.cursor.position(), (0, 0));
        assert_ne!(
            session.pooled(&ElementId::from("virtual-b"), None).unwrap(),
            retained
        );
    });
}
