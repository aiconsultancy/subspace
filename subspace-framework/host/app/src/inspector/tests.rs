//! The inspector's interaction in a window.

mod model;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
    Keystroke, ParentElement as _, Pixels, Point, Render, Styled as _, TestAppContext,
    TestSupportExt as _, Window, div, point, px,
};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::{prefs, secondary};
use subspace_lucide::IconName;

use super::{Reported, ToggleInspector, is_open};

struct Fixture;

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("outer")
            .test_support()
            .size_full()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .id("inner")
                    .test_support()
                    .w(px(60.))
                    .h(px(30.))
                    .p(px(4.))
                    .child(div().id("leaf").test_support().size(px(10.))),
            )
            .child(
                IconButton::new("fixture-button")
                    .icon(IconName::Copy)
                    .accessibility_label("Copy"),
            )
    }
}

fn open(cx: &mut TestAppContext, developer: bool) -> (AnyWindowHandle, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        if !developer {
            prefs::set(cx, crate::settings::DEVELOPER_MODE_KEY, "0");
        }
        super::init(cx);
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Fixture);
        gpui_kit::base::Root::new(view, window, cx)
    });
    let window: AnyWindowHandle = window.into();
    frame(window, cx);
    (window, root)
}

fn frame(window: AnyWindowHandle, cx: &mut TestAppContext) {
    for _ in 0..2 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

fn dispatch_toggle(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(ToggleInspector), cx);
    })
    .unwrap();
    cx.run_until_parked();
    frame(window, cx);
}

fn hover(window: AnyWindowHandle, at: Point<Pixels>, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.simulate_mouse_move(at, cx))
        .unwrap();
    frame(window, cx);
}

fn reported(cx: &mut TestAppContext) -> super::model::ElementReport {
    cx.update(|cx| cx.global::<Reported>().0.clone())
        .expect("the panel reports the active element")
}

fn bounds_of(window: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| window.find(id).bounds())
        .unwrap()
}

fn open_state(window: AnyWindowHandle, cx: &mut TestAppContext) -> (bool, bool) {
    cx.update_window(window, |_, window, cx| {
        (is_open(window, cx), window.is_inspector_picking(cx))
    })
    .unwrap()
}

#[gpui_kit::test]
fn hovering_picks_the_innermost_element_and_reports_its_bounds(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, true);
    dispatch_toggle(window, cx);
    assert_eq!(open_state(window, cx), (true, true));

    hover(window, point(px(70.), px(40.)), cx);
    let inner = reported(cx);
    assert_eq!(inner.element_id.as_deref(), Some("inner"));
    assert_eq!(inner.layout.bounds, bounds_of(window, "inner", cx));
    assert_eq!(inner.layout.padding.left, px(4.));
    assert!(inner.source.starts_with("host/app/src/inspector/tests.rs:"));

    hover(window, point(px(27.), px(27.)), cx);
    assert_eq!(reported(cx).element_id.as_deref(), Some("leaf"));

    hover(window, point(px(200.), px(300.)), cx);
    assert_eq!(reported(cx).element_id.as_deref(), Some("outer"));
}

#[gpui_kit::test]
fn a_shared_button_is_named_after_its_catalogue_component(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, true);
    dispatch_toggle(window, cx);
    let button = bounds_of(window, "fixture-button", cx);
    hover(window, button.center(), cx);
    let report = reported(cx);
    assert_eq!(report.name().as_deref(), Some("icon_button::IconButton"));
    assert!(report.tag().starts_with("icon_button::IconButton  "));
    assert!(report.chain().contains("icon_button::IconButton"));
}

#[gpui_kit::test]
fn clicking_pins_the_element_and_copy_writes_its_name(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, true);
    dispatch_toggle(window, cx);
    let button = bounds_of(window, "fixture-button", cx);
    hover(window, button.center(), cx);
    cx.update_window(window, |_, window, cx| window.click("fixture-button", cx))
        .unwrap();
    frame(window, cx);
    assert_eq!(
        open_state(window, cx),
        (true, false),
        "a click pins the element"
    );

    hover(window, point(px(200.), px(300.)), cx);
    assert_eq!(
        reported(cx).name().as_deref(),
        Some("icon_button::IconButton"),
        "a pinned element stays selected"
    );
    cx.update_window(window, |_, window, cx| {
        window.click("inspector-copy-name", cx)
    })
    .unwrap();
    cx.run_until_parked();
    let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("icon_button::IconButton"));
}

#[gpui_kit::test]
fn escape_and_the_toggle_close_the_inspector(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, true);
    dispatch_toggle(window, cx);
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    frame(window, cx);
    assert_eq!(open_state(window, cx), (false, false));

    dispatch_toggle(window, cx);
    assert!(open_state(window, cx).0);
    dispatch_toggle(window, cx);
    assert_eq!(open_state(window, cx), (false, false));
}

#[gpui_kit::test]
fn the_shortcut_opens_the_inspector(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, true);
    let key = if cfg!(target_os = "macos") {
        "cmd-f12"
    } else {
        "ctrl-f12"
    };
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .unwrap();
    cx.run_until_parked();
    frame(window, cx);
    assert!(open_state(window, cx).0);
}

#[gpui_kit::test]
fn developer_mode_off_keeps_the_inspector_and_its_commands_away(cx: &mut TestAppContext) {
    let (window, _root) = open(cx, false);
    dispatch_toggle(window, cx);
    assert_eq!(open_state(window, cx), (false, false));
    assert!(cx.update(|cx| super::palette_commands(cx).is_empty()));
}

#[gpui_kit::test]
fn the_developer_menu_and_palette_offer_the_inspector(cx: &mut TestAppContext) {
    let menu = super::menu();
    assert_eq!(menu.name.as_ref(), "Developer");
    assert_eq!(menu.items.len(), 3);
    let (window, _root) = open(cx, true);
    let labels = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            super::palette_commands(cx)
                .into_iter()
                .map(|command| command.label.to_string())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(labels(cx), ["Inspect elements", "Developer settings"]);
    dispatch_toggle(window, cx);
    assert_eq!(labels(cx)[0], "Close inspector");
}

#[gpui_kit::test]
fn the_inspector_key_leaves_f12_to_the_twelfth_space(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::app::bind_keys(cx);
        super::init(cx);
        let os = cx.platform_info().os;
        let toggles = |keys: &str, cx: &mut gpui_kit::App| {
            let input = [Keystroke::parse_with_os(keys, os).unwrap()];
            cx.all_bindings_for_input(&input)
                .iter()
                .any(|binding| binding.action().partial_eq(&ToggleInspector))
        };
        assert!(toggles(secondary!("f12"), cx));
        assert!(!toggles("f12", cx), "F12 still shows the twelfth Space");
    });
}
