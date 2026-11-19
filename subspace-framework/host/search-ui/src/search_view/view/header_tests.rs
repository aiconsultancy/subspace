use std::{cell::Cell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px, size};

use crate::full_text::Service;
use crate::search_view::{SearchView, SearchViewEvent};

#[gpui_kit::test]
fn the_shared_header_closes_search_by_pointer_and_escape(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(
        root.path().to_owned(),
        "test-app".into(),
        vec!["test-board".into()],
    )
    .unwrap();
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        SearchView::new(
            service,
            vec![("test-board".into(), "Test board".into())],
            "test-board".into(),
            window,
            cx,
        )
    });
    let closed = Rc::new(Cell::new(0));
    let observed = closed.clone();
    cx.update(|window, cx| {
        window
            .subscribe(&view, cx, move |_, event: &SearchViewEvent, _, _| {
                if matches!(event, SearchViewEvent::Close) {
                    observed.set(observed.get() + 1);
                }
            })
            .detach();
    });
    cx.simulate_resize(size(px(420.), px(500.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let header = window.find("surface-header").bounds();
        let sources = window.find("search-sources").bounds();
        let close = window.find("surface-close");
        assert_eq!(close.label(), Some("Close search"));
        assert!(sources.right() <= close.bounds().left());
        assert!(close.bounds().right() <= header.right());
        window.click("surface-close", cx);
    });
    cx.run_until_parked();
    assert_eq!(closed.get(), 1);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.focus(window, cx));
        window.render_frame(cx);
        window.press("escape", cx);
    });
    cx.run_until_parked();
    assert_eq!(closed.get(), 2);
}
