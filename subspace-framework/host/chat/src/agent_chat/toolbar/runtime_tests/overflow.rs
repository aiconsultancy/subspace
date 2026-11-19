use super::{ChatToolbar, ToolbarEvent, record_events, settle, setup};
use crate::agent_chat::test_menu;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, size};
use subspace_dashboard::icon_button::IconButtonSize;

fn narrow(cx: &mut TestAppContext) -> (Entity<ChatToolbar>, &mut VisualTestContext) {
    let (toolbar, cx) = setup(cx);
    cx.update(|_, cx| cx.set_global(IconButtonSize::ExtraLarge));
    toolbar.update(cx, |toolbar, cx| {
        let mut model = toolbar.model().clone();
        model.copilot_controls = true;
        model.models[0].efforts = vec!["low".into(), "high".into()];
        model.model = Some("test-model".into());
        toolbar.set_model(model, cx);
    });
    cx.simulate_resize(size(px(300.), px(600.)));
    settle(cx);
    (toolbar, cx)
}

fn choose(label: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    cx.update(|window, cx| test_menu::click(window, label, cx));
    settle(cx);
}

#[gpui_kit::test]
fn extra_large_composer_fits_one_row_and_keeps_send_stop_immediate(cx: &mut TestAppContext) {
    let (toolbar, cx) = narrow(cx);
    let (events, _subscription) = record_events(&toolbar, cx);
    cx.update(|window, cx| {
        let row = window.find("chat-composer-toolbar").bounds();
        let send = window.find("chat-send").bounds();
        let more = window.find("overflow-toolbar-button").bounds();
        assert_eq!(send.size.height, px(72.));
        assert!(send.right() <= row.right());
        assert!(more.left() >= row.left());
        assert_eq!(send.center().y, more.center().y);
        assert!(send.top() >= row.top() && send.bottom() <= row.bottom());
        assert!(window.try_find("chat-model").is_none());
        window.click("chat-send", cx);
    });
    toolbar.update(cx, |toolbar, cx| {
        let mut model = toolbar.model().clone();
        model.busy = true;
        toolbar.set_model(model, cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find("chat-send").label(), Some("Stop"));
        window.click("chat-send", cx);
    });
    assert_eq!(*events.borrow(), [ToolbarEvent::Send, ToolbarEvent::Stop]);
}

#[gpui_kit::test]
fn overflow_model_search_survives_reopen_and_escape_clears_then_dismisses(cx: &mut TestAppContext) {
    let (toolbar, cx) = narrow(cx);
    choose("Model", cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-model-body").visible());
        window.input("test", cx);
    });
    settle(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("chat-model-body").is_none());
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
    });
    cx.update(|window, cx| toolbar.update(cx, |toolbar, cx| toolbar.open_models(window, cx)));
    settle(cx);
    toolbar.read_with(cx, |toolbar, cx| {
        assert_eq!(toolbar.model_search.read(cx).query(cx).as_ref(), "test");
    });
    cx.update(|window, cx| {
        assert!(window.find("chat-model-body").visible());
        window.press("escape", cx);
    });
    settle(cx);
    // Pinned Command clears a non-empty query before it permits dismissal.
    toolbar.read_with(cx, |toolbar, cx| {
        assert!(toolbar.model_search.read(cx).query(cx).is_empty());
    });
    cx.update(|window, cx| {
        assert!(window.find("chat-model-body").visible());
        window.press("escape", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("chat-model-body").is_none());
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn overflow_picker_confirm_and_usage_reopen_keep_owner_callbacks(cx: &mut TestAppContext) {
    let (toolbar, cx) = narrow(cx);
    let (events, _subscription) = record_events(&toolbar, cx);
    choose("Model", cx);
    cx.update(|window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    });
    settle(cx);
    assert_eq!(
        *events.borrow(),
        [ToolbarEvent::SetModel {
            model: "test-model".into()
        }]
    );
    choose("Usage", cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-usage-body").visible());
        window.press("escape", cx);
    });
    settle(cx);
    choose("Usage", cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-usage-body").visible());
        toolbar.update(cx, |toolbar, cx| toolbar.dismiss_popups(window, cx));
    });
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("chat-usage-body").is_none()));
}

#[gpui_kit::test]
fn overflow_native_reasoning_picker_confirms_and_reopens(cx: &mut TestAppContext) {
    let (toolbar, cx) = narrow(cx);
    let (events, _subscription) = record_events(&toolbar, cx);
    choose("Reasoning", cx);
    cx.update(|window, cx| test_menu::click(window, "high", cx));
    settle(cx);
    assert_eq!(
        *events.borrow(),
        [ToolbarEvent::SetEffort(Some("high".into()))]
    );
    toolbar.update(cx, |toolbar, cx| {
        let mut model = toolbar.model().clone();
        model.effort = Some("high".into());
        toolbar.set_model(model, cx);
    });
    settle(cx);
    choose("Reasoning", cx);
    cx.update(|window, cx| {
        let ix = test_menu::item_index(window, "high");
        assert!(window.within("popup-menu").find(ix).visible());
        window.press("escape", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true));
    });
}
