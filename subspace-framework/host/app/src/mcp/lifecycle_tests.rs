//! Reconciles that arrive while a start waits for its reply.

use std::sync::Arc;

use gpui_kit::{App, TestAppContext};
use subspace_agent_tools::{
    Access, AgentTool, ApprovalPolicy, Arguments, ReadService, ReadTool, ToolHost, ToolSpec,
    test_support,
};
use subspace_core::config::{Config, McpSettings};
use tokio::sync::mpsc::UnboundedReceiver;

use super::worker::{Command, Worker};
use super::{McpService, ServerState, finish_start, reconcile, state};

fn settings(enabled: bool) -> McpSettings {
    let mut settings = McpSettings::default();
    settings.enabled = enabled;
    settings.port = 0;
    settings
}

/// Installs the service with a held worker and returns its command queue.
fn install(root: &std::path::Path, cx: &mut App) -> UnboundedReceiver<Command> {
    crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
    let mut service = McpService::new(&[], test_support::environment(Config::default()));
    // Help is an Application tool, so the catalog is not empty.
    service.boards = vec![subspace_mcp::Board {
        id: "test",
        title: "Test",
        description: "",
        help: "# Test",
        help_images: &[],
        tools: |_| Vec::new(),
    }];
    let (worker, commands) = Worker::held().unwrap();
    service.worker = Some(worker);
    cx.set_global(service);
    commands
}

/// The generation of the start that the last reconcile sent.
fn pending_start(cx: &App) -> u64 {
    cx.global::<McpService>().generation
}

/// The kinds of the commands sent since the last call.
fn sent(commands: &mut UnboundedReceiver<Command>) -> Vec<&'static str> {
    std::iter::from_fn(|| commands.try_recv().ok())
        .map(|command| match command {
            Command::Start { .. } => "start",
            Command::Swap(_) => "swap",
            Command::Stop(_) => "stop",
        })
        .collect()
}

#[gpui_kit::test]
fn disabling_during_a_pending_start_stops_it_and_ignores_its_reply(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        let mut commands = install(root.path(), cx);
        reconcile(&settings(true), cx);
        assert_eq!(state(cx), ServerState::Off, "the start waits for its reply");
        assert_eq!(sent(&mut commands), ["start"]);
        let start = pending_start(cx);

        reconcile(&settings(false), cx);
        assert_eq!(sent(&mut commands), ["stop"]);
        finish_start(start, Ok(([127, 0, 0, 1], 7331).into()), cx);

        assert_eq!(state(cx), ServerState::Off);
    });
}

#[gpui_kit::test]
fn only_the_latest_pending_start_settles(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        let mut commands = install(root.path(), cx);
        reconcile(&settings(true), cx);
        let first = pending_start(cx);
        // From Off, a pending start decides as Off, so it starts again.
        reconcile(&settings(true), cx);
        let second = pending_start(cx);
        assert_ne!(second, first);
        assert_eq!(sent(&mut commands), ["start", "start"]);

        finish_start(first, Ok(([127, 0, 0, 1], 7331).into()), cx);
        assert_eq!(state(cx), ServerState::Off);
        let address = ([127, 0, 0, 1], 7400).into();
        finish_start(second, Ok(address), cx);
        assert_eq!(state(cx), ServerState::Listening(address));
    });
}

/// A board whose tools need the running app, as IRC's do.
fn live_tools(environment: &subspace_agent_tools::ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    match environment.host {
        ToolHost::App => vec![Arc::new(ReadTool::new(
            ToolSpec {
                name: "live_read",
                title: "Read live",
                description: "Read live",
                access: Access::Read,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::None,
            },
            Arc::new(Live),
        ))],
        ToolHost::Headless => Vec::new(),
    }
}

struct Live;

impl ReadService for Live {
    type Request = serde_json::Value;
    fn read(&self, request: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(request)
    }
}

#[gpui_kit::test]
fn acp_sessions_get_the_in_app_tools_whatever_settings_mcp_serves(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        install(root.path(), cx);
        let service = cx.global_mut::<McpService>();
        service.boards[0].tools = live_tools;
        service.environment = test_support::app_environment(Config::default());
        let mut off = settings(false);
        off.groups.insert("test".into(), false);
        reconcile(&off, cx);
        let context = super::chat_tools(cx).unwrap().instructions();
        assert!(
            context.contains("- Test (test): Tools: live_read (read)."),
            "{context}"
        );
        let headless = test_support::environment(Config::default());
        let boards = cx.global::<McpService>().boards.clone();
        let served = subspace_mcp::served_catalog(&boards, &[], &headless).unwrap();
        assert!(
            served.get("live_read").is_none(),
            "headless omits live tools"
        );
    });
}
