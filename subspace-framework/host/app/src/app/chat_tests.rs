//! Open in chat on a cloud agent task that the chat pane cannot replay: a
//! toast says why, whether the pane is off or shows its Sessions tab.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render,
    TestAppContext, Window, WindowHandle, div, px, size,
};
use subspace_agent_tools::Catalog;
use subspace_chat::test_support::{AgentHandle, Command};
use subspace_core::config::{ChatEngine, Config};
use subspace_core::credentials::Credentials;
use subspace_dashboard::{ChatRequest, CloudSession, Dashboard, DashboardEvent, Services, prefs};

use super::{SubspaceApp, ToggleChat};
use crate::registry::{BoardFactories, BoardRegistration};

struct Board {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for Board {}

impl Focusable for Board {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Board {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Dashboard for Board {
    const ID: &'static str = "cloud-replay";
    const TITLE: &'static str = "Cloud replay";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

/// The shell with chat configured for `engine`, as the config file sets it.
fn open(
    engine: ChatEngine,
    cx: &mut TestAppContext,
) -> (WindowHandle<SubspaceApp>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, &path, path.join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "0");
        cx.set_global(BoardFactories(vec![BoardRegistration::new::<Board>()]));
        let mut config = Config::default();
        config.chat.engine = engine;
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: path.join("config.json"),
        });
    });
    let window = cx.open_window(size(px(1_200.), px(900.)), |window, cx| {
        SubspaceApp::new(Vec::new(), window, cx)
    });
    settle(window, cx);
    (window, root)
}

fn settle(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
    for _ in 0..6 {
        cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

fn open_in_chat(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
    let task = CloudSession {
        task_id: "fixture-cloud-task".into(),
        title: "Cloud fixture".into(),
        url: "https://github.com/example-org/example-app/tasks/fixture-cloud-task".into(),
    };
    window
        .update(cx, |app, window, cx| {
            app.chat_request(&ChatRequest::OpenCloudSession(task), window, cx);
        })
        .unwrap();
    settle(window, cx);
}

fn toasts(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> Vec<String> {
    window
        .read_with(cx, |app, _| {
            app.toasts
                .iter()
                .map(|shown| shown.toast.text.to_string())
                .collect()
        })
        .unwrap()
}

/// The reason this build gives when Pi, ACP or (without Copilot) Chat off
/// cannot replay a cloud session.
fn refusal_names_this_build(refusal: &str) -> bool {
    if subspace_chat::COPILOT_BUILT_IN {
        refusal.contains("Choose Copilot in Settings")
    } else {
        refusal.contains("this build of Subspace does not include")
    }
}

#[gpui_kit::test]
fn open_in_chat_with_chat_off_says_why_unless_copilot_could_be_chosen(cx: &mut TestAppContext) {
    let (window, _root) = open(ChatEngine::Off, cx);
    open_in_chat(window, cx);
    let shown = toasts(window, cx);
    let (chat_open, off) = window
        .read_with(cx, |app, cx| {
            (app.chat_open, app.chat.read(cx).is_disabled())
        })
        .unwrap();
    assert!(off);
    if subspace_chat::COPILOT_BUILT_IN {
        assert!(chat_open, "the pane opens and says how to turn chat on");
        assert!(shown.is_empty(), "{shown:?}");
    } else {
        assert!(!chat_open, "nothing in the pane could replay it");
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert!(refusal_names_this_build(&shown[0]), "{shown:?}");
    }
}

#[gpui_kit::test]
fn open_in_chat_from_pi_on_the_sessions_tab_says_why_in_a_toast(cx: &mut TestAppContext) {
    let (window, root) = open(ChatEngine::Pi, cx);
    let (handle, mut commands) =
        AgentHandle::test_offline_engine(ChatEngine::Pi, root.path().into(), Vec::new());
    window
        .update(cx, |app, window, cx| {
            app.chat.update(cx, |chat, _| chat.set_test_runtime(handle));
            app.toggle_chat(&ToggleChat, window, cx);
        })
        .unwrap();
    settle(window, cx);
    // The Sessions tab replaces the open chat and its composer.
    let composer_shown = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, _| {
            assert!(window.try_find("agent-chat").is_some(), "the pane is open");
            window.try_find("chat-input").is_some()
        })
        .unwrap()
    };
    assert!(composer_shown(cx));
    window
        .update(cx, |app, window, cx| {
            app.chat
                .update(cx, |chat, cx| chat.show_test_sessions(window, cx));
        })
        .unwrap();
    settle(window, cx);
    assert!(!composer_shown(cx), "the Sessions tab is shown");
    while commands.try_recv().is_ok() {}

    open_in_chat(window, cx);

    let shown = toasts(window, cx);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(refusal_names_this_build(&shown[0]), "{shown:?}");
    assert!(!composer_shown(cx), "the pane stays on the Sessions tab");
    assert!(
        !std::iter::from_fn(|| commands.try_recv().ok())
            .any(|command| matches!(command, Command::OpenRemote { .. })),
        "Pi is not asked to replay it"
    );
}
