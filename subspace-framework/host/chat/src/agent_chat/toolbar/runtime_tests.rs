use super::{ChatToolbar, ModelOption, ToolbarEvent, ToolbarModel};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Subscription, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

mod overflow;

fn setup(cx: &mut TestAppContext) -> (Entity<ChatToolbar>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (toolbar, cx) = cx.add_window_view(ChatToolbar::new);
    toolbar.update(cx, |toolbar, cx| {
        toolbar.set_model(
            ToolbarModel {
                models: vec![ModelOption {
                    id: "test-model".into(),
                    label: "Test model".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            cx,
        );
    });
    settle(cx);
    (toolbar, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn record_events(
    toolbar: &Entity<ChatToolbar>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<ToolbarEvent>>>, Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(toolbar, move |_, event, _| {
            received.borrow_mut().push(event.clone());
        })
    });
    (events, subscription)
}

#[gpui_kit::test]
fn usage_body_toggles_and_escape_returns_focus(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    cx.update(|window, cx| window.click("chat-usage", cx));
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-usage-body").visible());
        window.click("chat-usage", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("chat-usage-body").is_none());
        window.click("chat-usage", cx);
        window.press("escape", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("chat-usage-body").is_none());
        assert_eq!(window.find("chat-usage").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn model_body_confirms_selection_and_closes(cx: &mut TestAppContext) {
    let (toolbar, cx) = setup(cx);
    let (events, _subscription) = record_events(&toolbar, cx);
    cx.update(|window, cx| window.click("chat-model", cx));
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-model-body").visible());
        window.press("down", cx);
        window.press("enter", cx);
    });
    settle(cx);
    assert_eq!(
        *events.borrow(),
        vec![ToolbarEvent::SetModel {
            model: "test-model".into()
        }]
    );
    cx.update(|window, _| {
        assert!(window.try_find("chat-model-body").is_none());
        assert_eq!(window.find("chat-model").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn model_search_survives_reopen_and_owner_dismisses_both_surfaces(cx: &mut TestAppContext) {
    let (toolbar, cx) = setup(cx);
    cx.update(|window, cx| window.click("chat-model", cx));
    settle(cx);
    cx.update(|window, cx| {
        window.input("test", cx);
        window.click("chat-model-star-test-model", cx);
    });
    settle(cx);
    toolbar.read_with(cx, |toolbar, _| {
        assert_eq!(toolbar.favourites, ["test-model"])
    });
    cx.update(|window, cx| window.click("chat-model", cx));
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("chat-model-body").is_none()));
    cx.update(|window, cx| toolbar.update(cx, |toolbar, cx| toolbar.open_models(window, cx)));
    settle(cx);
    cx.update(|window, _| assert!(window.find("chat-model-body").visible()));
    toolbar.read_with(cx, |toolbar, cx| {
        assert_eq!(toolbar.model_search.read(cx).query(cx).as_ref(), "test");
    });
    cx.update(|window, cx| toolbar.update(cx, |toolbar, cx| toolbar.dismiss_popups(window, cx)));
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("chat-model-body").is_none());
        window.click("chat-usage", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-usage-body").visible());
        toolbar.update(cx, |toolbar, cx| toolbar.dismiss_popups(window, cx));
    });
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("chat-usage-body").is_none()));
}

#[gpui_kit::test]
fn composer_button_sends_then_stops_when_busy(cx: &mut TestAppContext) {
    let (toolbar, cx) = setup(cx);
    let (events, _subscription) = record_events(&toolbar, cx);
    cx.update(|window, cx| window.click("chat-send", cx));
    toolbar.update(cx, |toolbar, cx| {
        let mut model = toolbar.model().clone();
        model.busy = true;
        toolbar.set_model(model, cx);
    });
    settle(cx);
    cx.update(|window, cx| window.click("chat-send", cx));
    assert_eq!(
        *events.borrow(),
        vec![ToolbarEvent::Send, ToolbarEvent::Stop]
    );
}

#[gpui_kit::test]
fn composer_follows_global_size_but_model_favourite_stays_compact(cx: &mut TestAppContext) {
    use gpui_kit::px;
    use subspace_dashboard::icon_button::IconButtonSize;

    let (_, cx) = setup(cx);
    cx.update(|_, cx| cx.set_global(IconButtonSize::ExtraLarge));
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find("chat-send").bounds().size.height, px(72.));
        window.click("chat-model", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window
                .find("chat-model-star-test-model")
                .bounds()
                .size
                .height,
            px(20.)
        );
    });
}
