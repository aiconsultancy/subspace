use super::external;
use crate::Tokens;
use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, ClipboardItem, Context, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, WindowBounds, WindowOptions, div, px, size,
};

const WEB_URL: &str = "https://example.com/releases?product=subspace";

struct ExternalLinkFixture {
    url: &'static str,
    focus: FocusHandle,
}

impl Render for ExternalLinkFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .tab_group()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .child(
                // Base Link is RenderOnce, so observe its tight host instead.
                // Pointer and keyboard events still reach the production link.
                div()
                    .id("external-link-host")
                    .test_support()
                    .flex()
                    .child(external(
                        "external-link",
                        "Project repository",
                        self.url,
                        &Tokens::one_dark(),
                    )),
            )
    }
}

fn mount(cx: &mut TestAppContext, url: &'static str) -> VisualTestContext {
    cx.update(gpui_kit::init);
    let (window, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(640.), px(480.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    ExternalLinkFixture { url, focus }
                })
            },
        )
        .expect("open external link test window")
    });
    let mut cx = VisualTestContext::from_window(window, cx);
    settle(&mut cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

#[gpui_kit::test]
fn pointer_opens_the_production_external_link(cx: &mut TestAppContext) {
    let mut cx = mount(cx, " https://example.com/releases?product=subspace ");
    assert_eq!(cx.opened_url(), None);
    cx.update(|window, cx| {
        let host = window.find("external-link-host");
        assert!(host.visible());
        assert!(host.bounds().size.width > px(0.));
        assert!(host.bounds().size.height > px(0.));
        window.click("external-link-host", cx);
    });
    assert_eq!(cx.opened_url().as_deref(), Some(WEB_URL));
}

#[gpui_kit::test]
fn enter_opens_a_keyboard_focused_external_link(cx: &mut TestAppContext) {
    let mut cx = mount(cx, WEB_URL);
    cx.update(|window, cx| {
        window.press("tab", cx);
        assert!(window.focused(cx).is_some());
        window.press("enter", cx);
    });
    assert_eq!(cx.opened_url().as_deref(), Some(WEB_URL));
}

#[gpui_kit::test]
fn space_opens_a_keyboard_focused_mail_link(cx: &mut TestAppContext) {
    let mut cx = mount(cx, "mailto:maintainer@example.com");
    cx.update(|window, cx| {
        window.press("tab", cx);
        assert!(window.focused(cx).is_some());
        window.press("space", cx);
    });
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("mailto:maintainer@example.com")
    );
}

#[gpui_kit::test]
fn pointer_and_keyboard_cannot_open_an_unsafe_destination(cx: &mut TestAppContext) {
    let mut cx = mount(cx, "javascript:alert('test-only')");
    cx.update(|window, cx| {
        window.click("external-link-host", cx);
        assert!(window.focused(cx).is_some());
        window.press("enter", cx);
        window.press("space", cx);
    });
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn right_click_copies_the_destination_without_opening_it(cx: &mut TestAppContext) {
    let mut cx = mount(cx, WEB_URL);
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        window.right_click("external-link-host", cx);
    });
    settle(&mut cx);
    assert_eq!(cx.opened_url(), None);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("unchanged".into())
    );
    cx.update(|window, cx| {
        assert_eq!(window.find(0usize).label(), Some("Copy link"));
        window.click(0usize, cx);
    });
    settle(&mut cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(WEB_URL.into())
    );
    assert_eq!(cx.opened_url(), None);
    cx.update(|window, _| assert!(window.try_find(0usize).is_none()));
}
