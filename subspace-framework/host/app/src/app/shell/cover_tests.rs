use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, Render, TestAppContext, Window, div,
};
use subspace_dashboard::{Dashboard, DashboardEvent};

use super::SubspaceApp;

/// A board that records whether the shell says something covers it.
struct Fixture {
    focus: FocusHandle,
    covered: bool,
}
impl EventEmitter<DashboardEvent> for Fixture {}
impl Focusable for Fixture {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(Self::ID)
            .key_context(Self::ID)
            .track_focus(&self.focus)
    }
}
impl Dashboard for Fixture {
    const ID: &'static str = "cover-fixture";
    const TITLE: &'static str = "Cover fixture";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            covered: false,
        }
    }
    fn covered(&mut self, covered: bool, _: &mut Window, _: &mut Context<Self>) {
        self.covered = covered;
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn open_fixture(
    root: &std::path::Path,
    cx: &mut TestAppContext,
) -> (
    gpui_kit::WindowHandle<SubspaceApp>,
    gpui_kit::Entity<Fixture>,
) {
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        cx.set_global(crate::registry::BoardFactories(vec![
            crate::registry::BoardRegistration::new::<Fixture>(),
        ]));
    });
    let window = cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx));
    let fixture = window
        .update(cx, |app, window, cx| {
            app.focus_active(window, cx);
            app.dashboard()
                .unwrap()
                .view()
                .downcast::<Fixture>()
                .unwrap_or_else(|_| panic!("the active dashboard is the fixture"))
        })
        .unwrap();
    (window, fixture)
}

/// A board that owns a native view must hear when shell overlays cover
/// it, because the native view would otherwise draw above them.
#[gpui_kit::test]
fn shell_overlays_cover_and_uncover_the_boards(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, fixture) = open_fixture(root.path(), cx);
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_root, window, cx| window.render_frame(cx))
            .unwrap();
    };
    draw(cx);
    assert!(!fixture.read_with(cx, |fixture, _| fixture.covered));

    window
        .update(cx, |app, window, cx| app.set_jig(true, window, cx))
        .unwrap();
    draw(cx);
    assert!(fixture.read_with(cx, |fixture, _| fixture.covered));

    window
        .update(cx, |app, window, cx| app.set_jig(false, window, cx))
        .unwrap();
    draw(cx);
    assert!(!fixture.read_with(cx, |fixture, _| fixture.covered));
}

/// A maximized terminal covers the Space, so a board with a native view
/// hides it, and restoring uncovers it.
#[gpui_kit::test]
fn a_maximized_terminal_covers_the_boards(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(crate::app::terminal_fixture::install);
    let (window, fixture) = open_fixture(root.path(), cx);
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_root, window, cx| window.render_frame(cx))
            .unwrap();
    };
    window
        .update(cx, |app, window, cx| {
            app.change_terminal(
                super::super::terminal_dock::Change::ToggleMaximized,
                window,
                cx,
            );
        })
        .unwrap();
    draw(cx);
    assert!(fixture.read_with(cx, |fixture, _| fixture.covered));
    window
        .update(cx, |app, window, cx| {
            app.change_terminal(super::super::terminal_dock::Change::Restore, window, cx);
        })
        .unwrap();
    draw(cx);
    assert!(!fixture.read_with(cx, |fixture, _| fixture.covered));
}

/// An app that requires its account covers the window with sign-in while
/// there is no session, covering the Space and taking focus, and opens
/// again once signed in or with a saved session it cannot renew.
#[gpui_kit::test]
fn sign_in_covers_the_window_only_while_a_required_account_has_no_session(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, fixture) = open_fixture(root.path(), cx);
    let gate = |state: &str, cx: &mut TestAppContext| {
        cx.update(|cx| crate::account::fixture(state, cx).unwrap());
        cx.update_window(window.into(), |_root, window, cx| {
            window.refresh();
            window.render_frame(cx);
            window.try_find("sign-in-gate").is_some()
        })
        .unwrap()
    };
    // No account, or one the app does not require: never.
    cx.update_window(window.into(), |_root, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("sign-in-gate").is_none());
    })
    .unwrap();
    assert!(!gate("signed-out", cx));
    // Required with no session: the gate, over a covered Space, with focus
    // on its sign-in.
    assert!(gate("required-signed-out", cx));
    assert!(fixture.read_with(cx, |fixture, _| fixture.covered));
    cx.update_window(window.into(), |_root, window, cx| {
        assert_eq!(
            window.find("sign-in-gate-sign-in").label(),
            Some("Sign in with Example Identity")
        );
        assert!(window.try_find("sign-in-gate-quit").is_some());
        let focused = window.focused(cx);
        let gate = cx
            .global::<crate::app::sign_in_gate::SignInGate>()
            .focus
            .clone();
        assert!(focused.is_some() && focused == gate, "focus is on sign-in");
    })
    .unwrap();
    // While the browser leg runs it stays, with Cancel and the browser.
    assert!(gate("required-waiting", cx));
    // An expired session asks again; signed in, or a saved session the
    // service cannot renew just now, opens the app.
    assert!(gate("required-expired", cx));
    assert!(!gate("required-signed-in", cx));
    assert!(!fixture.read_with(cx, |fixture, _| fixture.covered));
    assert!(!gate("required-offline", cx));
    assert!(!gate("required-keychain", cx));
}
