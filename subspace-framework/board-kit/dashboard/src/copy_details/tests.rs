use super::{CopyDetails, CopyDiagnostics};
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ClipboardItem, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Role, Styled as _, TestAppContext, VisualTestContext,
    Window, WindowOptions, div,
};
use subspace_core::diagnostics::{BuildIdentity, ErrorDetails, LocalDiagnostics};

struct CopyFixture {
    focus: FocusHandle,
    title: &'static str,
    message: &'static str,
    log_opens: usize,
}

fn mount(cx: &mut TestAppContext) -> (Entity<CopyFixture>, VisualTestContext) {
    let (window, fixture) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                CopyFixture {
                    focus,
                    title: "Fixture failed",
                    message: "Readable fixture cause",
                    log_opens: 0,
                }
            })
        })
        .expect("open copy details fixture")
    });
    (fixture, VisualTestContext::from_window(window, cx))
}

impl Render for CopyFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &crate::log_view::OpenLogView, _, cx| {
                    this.log_opens += 1;
                    cx.notify();
                }),
            )
            .tab_group()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .p_4()
            .child(CopyDetails::new(
                "error-copy",
                ErrorDetails::new(self.title, self.message, "fixture stage"),
            ))
            .child(
                CopyDetails::diagnostics("about-copy").with_diagnostics(LocalDiagnostics::new(
                    BuildIdentity::new("gallery-fixture-product", "0.0.0-fixture"),
                    "0.0.0-fixture",
                )),
            )
    }
}

#[gpui_kit::test]
fn pointer_copies_error_and_common_diagnostics_then_confirms(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(CopyDiagnostics::new(LocalDiagnostics::new(
            BuildIdentity::new("shell-fixture-product", "0.0.0-fixture"),
            "fixture-framework",
        )));
    });
    let (_, mut context) = mount(cx);
    let cx = &mut context;
    cx.update(|window, cx| {
        window.render_frame(cx);
        let button = window.find("error-copy");
        assert_eq!(button.role(), Some(Role::Button));
        assert_eq!(button.label(), Some("Copy details"));
        assert!(button.visible());
        window.click("error-copy", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("error-copy").label(), Some("Copy details"));
        assert_eq!(window.find("about-copy").label(), Some("Copy diagnostics"));
        assert_eq!(window.notifications(cx).len(), 1);
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(copied.contains("Fixture failed"), "{copied}");
        assert!(copied.contains("Readable fixture cause"), "{copied}");
        assert!(copied.contains("fixture stage"), "{copied}");
        assert!(copied.contains("shell-fixture-product"), "{copied}");
        assert!(copied.contains("fixture-framework"), "{copied}");
        assert!(copied.contains("not found on the app's PATH"), "{copied}");
    });
}

#[gpui_kit::test]
fn tab_enter_and_space_copy_through_native_buttons_without_error_leakage(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, mut context) = mount(cx);
    let cx = &mut context;
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.press("tab", cx);
        assert_eq!(window.find("error-copy").focused(), Some(true));
        window.press("enter", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let error = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(error.contains("Fixture failed"), "{error}");
        cx.write_to_clipboard(ClipboardItem::new_string("replace this fixture".into()));
        window.press("tab", cx);
        assert_eq!(window.find(show_log_id()).focused(), Some(true));
        window.press("tab", cx);
        assert_eq!(window.find("about-copy").focused(), Some(true));
        window.press("space", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("about-copy").label(), Some("Copy diagnostics"));
        assert_eq!(window.notifications(cx).len(), 1);
        let about = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(!about.contains("Fixture failed"), "{about}");
        assert!(!about.contains("replace this fixture"), "{about}");
        assert!(about.contains("gallery-fixture-product"), "{about}");
        assert!(about.contains("not found on the app's PATH"), "{about}");
    });
}

fn show_log_id() -> gpui_kit::ElementId {
    gpui_kit::ElementId::NamedChild(std::sync::Arc::new("error-copy".into()), "show-log".into())
}

#[gpui_kit::test]
fn error_actions_open_the_existing_log_command_by_pointer_and_keyboard(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, mut cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(show_log_id()).label(), Some("Show log"));
        assert_eq!(window.find(show_log_id()).role(), Some(Role::Button));
        window.click(show_log_id(), cx);
    });
    cx.run_until_parked();
    assert_eq!(fixture.read_with(&cx, |this, _| this.log_opens), 1);
    cx.update(|window, cx| {
        let focus = fixture.read(cx).focus.clone();
        focus.focus(window, cx);
        window.press("tab", cx);
        window.press("tab", cx);
        assert_eq!(window.find(show_log_id()).focused(), Some(true));
        window.press("space", cx);
    });
    cx.run_until_parked();
    assert_eq!(fixture.read_with(&cx, |this, _| this.log_opens), 2);
}

#[gpui_kit::test]
fn renders_and_copy_do_not_log_the_same_error_again(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, mut cx) = mount(cx);
    let title = "copy-control-render-does-not-log-fixture";
    fixture.update(&mut cx, |fixture, cx| {
        fixture.title = title;
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("error-copy", cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(gpui_kit::ElementId::NamedChild(
                    std::sync::Arc::new("about-copy".into()),
                    "show-log".into(),
                ))
                .is_none()
        );
    });
    assert_eq!(
        subspace_core::log::recent()
            .iter()
            .filter(|record| {
                record.message == title
                    || record
                        .fields
                        .iter()
                        .any(|(key, value)| key == "title" && value == title)
            })
            .count(),
        0
    );
}

#[gpui_kit::test]
fn replacing_error_under_the_same_id_does_not_claim_it_was_copied(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, mut cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("error-copy", cx);
    });
    cx.run_until_parked();
    fixture.update(&mut cx, |fixture, cx| {
        fixture.title = "Second fixture failed";
        fixture.message = "Second fixture cause";
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("error-copy").label(), Some("Copy details"));
        let previous = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(previous.contains("Readable fixture cause"), "{previous}");
        assert!(!previous.contains("Second fixture cause"), "{previous}");
        window.click("error-copy", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("error-copy").label(), Some("Copy details"));
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(copied.contains("Second fixture failed"), "{copied}");
        assert!(copied.contains("Second fixture cause"), "{copied}");
        assert!(!copied.contains("Readable fixture cause"), "{copied}");
    });
}
