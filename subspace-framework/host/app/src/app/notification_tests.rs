use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, Global, IntoElement, Render,
    SystemNotificationResponse, TestAppContext, Window, div,
};
use subspace_dashboard::notifications::{self, Mode, Notice, Preferences};
use subspace_dashboard::settings::BoardSettings;
use subspace_dashboard::{Dashboard, DashboardEvent, prefs};

#[derive(Default)]
struct Observed {
    activated: usize,
    targets: Vec<u64>,
}
impl Global for Observed {}
struct Fixture {
    focus: FocusHandle,
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
    }
}
impl Dashboard for Fixture {
    const ID: &'static str = "fixture";
    const TITLE: &'static str = "Fixture";
    fn settings() -> BoardSettings {
        BoardSettings {
            notifications: true,
            notification_defaults: Preferences {
                mode: Mode::System,
                ..Preferences::default()
            },
            ..BoardSettings::default()
        }
    }
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn activate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.global_mut::<Observed>().activated += 1;
    }
    fn open_notification(&mut self, target: u64, _: &mut Window, cx: &mut Context<Self>) {
        cx.global_mut::<Observed>().targets.push(target);
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

#[gpui_kit::test]
fn described_notices_post_who_and_where_and_a_click_opens_the_board(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        cx.set_app_identity("dev.subspace.fixture", "Subspace");
        cx.set_global(Observed::default());
        let boards = vec![crate::registry::BoardRegistration::new::<Fixture>()];
        crate::notifications::install_defaults(&boards, cx);
        cx.set_global(crate::registry::BoardFactories(boards));
        crate::notifications::init(cx);
    });
    let window = cx.add_window(|window, cx| super::SubspaceApp::new(Vec::new(), window, cx));
    cx.update(|cx| {
        assert_eq!(
            notifications::preferences("fixture", cx).mode,
            Mode::System,
            "the board's declared default applies before any choice is saved"
        );
        let described = Notice::described("alice mentioned you in #subspace", "Local Ergo");
        notifications::request("fixture", 7, described, cx);
        notifications::request("fixture", 8, Notice::new("Page text, never posted"), cx);
    });
    cx.run_until_parked();
    let posted = cx.shown_system_notifications();
    let texts: Vec<(&str, &str)> = posted
        .iter()
        .map(|post| (post.title.as_ref(), post.body.as_ref()))
        .collect();
    assert_eq!(
        texts,
        [
            ("alice mentioned you in #subspace", "Local Ergo"),
            ("Subspace", "New activity is available."),
        ]
    );
    cx.simulate_system_notification_response(SystemNotificationResponse {
        tag: posted[0].tag.clone(),
        action_id: None,
    });
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(cx.global::<Observed>().targets, [7]));
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}

#[gpui_kit::test]
fn pending_native_route_precedes_restore_and_unknown_route_covers_old_details(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "1");
        cx.set_global(Observed::default());
        cx.set_global(crate::registry::BoardFactories(vec![
            crate::registry::BoardRegistration::new::<Fixture>(),
        ]));
        crate::notifications::init(cx);
        cx.global_mut::<crate::notifications::Runtime>().pending =
            Some(crate::notifications::Route::Board("fixture", 42));
    });
    let window = cx.add_window(|window, cx| super::SubspaceApp::new(Vec::new(), window, cx));
    window
        .update(cx, |app, window, cx| {
            assert_eq!(cx.global::<Observed>().targets, [42]);
            assert_eq!(cx.global::<Observed>().activated, 0);
            assert!(!app.chat_open);
            assert!(!app.notification_unavailable);
            app.open_notification(crate::notifications::Route::Unavailable, window, cx);
            assert!(app.dashboard().is_none());
            assert!(app.notification_unavailable);
            window.remove_window();
        })
        .unwrap();
    cx.update(|cx| {
        assert!(cx.has_global::<crate::notifications::Runtime>());
        prefs::set(cx, "notifications.fixture.mode", "system");
        super::save_prefs(cx);
        assert_eq!(
            subspace_core::state::State::load(&root.path().join("state.json"))
                .get("notifications.fixture.mode"),
            Some("system")
        );
    });
}
