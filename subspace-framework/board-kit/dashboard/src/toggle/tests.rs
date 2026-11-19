use super::toggle_row;
use gpui_kit::component::{Disableable as _, switch::Switch};
use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowBounds, WindowOptions, div, px, size,
};

const LONG_LABEL: &str = "An independent option with a long label that must wrap in a narrow panel";

struct Options {
    checked: bool,
    disabled: bool,
    requests: Vec<bool>,
    focus: FocusHandle,
}

impl Render for Options {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .tab_group()
            .w_full()
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .child(
                toggle_row(
                    LONG_LABEL,
                    Switch::new("option")
                        .checked(self.checked)
                        .disabled(self.disabled)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.requests.push(*checked);
                            this.checked = *checked;
                            cx.notify();
                        })),
                )
                .id("option-row")
                .test_support(),
            )
            .child(toggle_row("Short", Switch::new("second-option")))
    }
}

fn mount(cx: &mut TestAppContext, disabled: bool) -> (VisualTestContext, Entity<Options>) {
    cx.update(gpui_kit::init);
    let (window, options) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(260.), px(480.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    Options {
                        checked: false,
                        disabled,
                        requests: Vec::new(),
                        focus,
                    }
                })
            },
        )
        .expect("open toggle test window")
    });
    let mut cx = VisualTestContext::from_window(window, cx);
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    (cx, options)
}

#[gpui_kit::test]
fn native_pointer_updates_the_controlled_value_and_accessible_state(cx: &mut TestAppContext) {
    let (mut cx, options) = mount(cx, false);
    cx.update(|window, cx| {
        assert_eq!(window.find("option").role(), Some(Role::Switch));
        assert_eq!(window.find("option").label(), Some(LONG_LABEL));
        assert_eq!(window.find("option").checked(), Some(false));
        window.click("option", cx);
        assert_eq!(window.find("option").checked(), Some(true));
        assert_eq!(options.read(cx).requests, vec![true]);
        window.click("option", cx);
        assert_eq!(window.find("option").checked(), Some(false));
        assert_eq!(options.read(cx).requests, vec![true, false]);
    });
}

#[gpui_kit::test]
fn tab_space_and_enter_use_the_native_switch_callback(cx: &mut TestAppContext) {
    let (mut cx, options) = mount(cx, false);
    cx.update(|window, cx| {
        window.press("tab", cx);
        assert_eq!(window.find("option").focused(), Some(true));
        window.press("space", cx);
        assert_eq!(window.find("option").checked(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find("option").checked(), Some(false));
        assert_eq!(options.read(cx).requests, vec![true, false]);
        window.press("tab", cx);
        assert_eq!(window.find("second-option").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn disabled_switch_rejects_pointer_and_keyboard_requests(cx: &mut TestAppContext) {
    let (mut cx, options) = mount(cx, true);
    cx.update(|window, cx| {
        window.click("option", cx);
        window.press("space", cx);
        window.press("enter", cx);
        assert!(!options.read(cx).checked);
        assert!(options.read(cx).requests.is_empty());
        assert_eq!(window.find("option").checked(), Some(false));
        window.press("tab", cx);
        assert_eq!(window.find("second-option").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn narrow_rows_wrap_labels_without_moving_the_trailing_switch_edge(cx: &mut TestAppContext) {
    let (mut cx, _) = mount(cx, false);
    cx.update(|window, _| {
        let row = window.find("option-row").bounds();
        let switch = window.find("option").bounds();
        let second = window.find("second-option").bounds();
        assert_eq!(switch.right(), row.right());
        assert_eq!(switch.right(), second.right());
        assert_eq!(switch.top(), row.top());
        assert!(row.size.height > switch.size.height);
        assert!(switch.left() > row.left());
        assert!(second.top() >= row.bottom());
    });
}
