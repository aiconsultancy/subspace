use super::LogView;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
mod close;
mod copy_path;
mod error_record;
mod toolbar;

#[gpui_kit::test]
fn actual_log_picker_selection_toggle_and_escape_use_retained_host(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
    });
    let (view, cx) = cx.add_window_view(LogView::new);
    cx.simulate_resize(gpui_kit::size(gpui_kit::px(2400.), gpui_kit::px(600.)));
    cx.update(|window, cx| {
        crate::bind_keys(cx);
        window.click("log-target", cx);
        assert_eq!(
            window.find("menu").bounds().top(),
            window.find("log-target").bounds().bottom()
        );
        window.press("1", cx);
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("menu").is_none());
        assert_eq!(window.find("log-target").focused(), Some(true));
        window.click("log-capture", cx);
    });
    cx.update(|window, cx| {
        window.click("log-capture", cx);
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("menu").is_none());
        window.click("log-target", cx);
        window.press("escape", cx);
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("menu").is_none());
    });
    view.read_with(cx, |view, _| {
        assert!(view.menu.is_none());
        assert!(view.filter.target.is_none());
    });
}
