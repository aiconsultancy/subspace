//! The app server (REQ-10 to REQ-16). One Tokio runtime hosts it; the GPUI
//! side decides what to do and records the state the MCP page shows.

mod decision;
#[cfg(test)]
mod lifecycle_tests;
mod worker;

use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, Global};
use subspace_agent_tools::{ChatToolServer, ToolEnvironment};
use subspace_core::config::McpSettings;
use subspace_core::mcp_token::{self, TokenStatus};
use subspace_dashboard::{CurrentProduct, Services, Toast};
use subspace_mcp::{
    ChatTools, GroupedCatalog, HostConfig, HttpOptions, Identity, PromptInput, ToolGroup,
};

pub(crate) use decision::ServerState;
use decision::{Decision, Lifecycle, Listener, Target, decide, token_reason};
use worker::{StartReply, Worker};

/// The app server and the facts the MCP page reads.
pub(crate) struct McpService {
    boards: Vec<subspace_mcp::Board>,
    environment: ToolEnvironment,
    worker: Option<Worker>,
    lifecycle: Lifecycle,
    applied: Option<Target>,
    /// Counts sent starts, so a reply settles only its own start.
    generation: u64,
    token_revision: u64,
    catalog: Option<GroupedCatalog>,
    toasts: Vec<Toast>,
}

impl Global for McpService {}

impl McpService {
    /// Tools are built from the startup environment, as for Subspace chat.
    pub(crate) fn new(boards: &[crate::BoardRegistration], environment: ToolEnvironment) -> Self {
        Self {
            boards: crate::registry::mcp_boards(boards),
            environment,
            worker: None,
            lifecycle: Lifecycle::Settled(ServerState::Off),
            applied: None,
            generation: 0,
            token_revision: 0,
            catalog: None,
            toasts: Vec::new(),
        }
    }

    /// The tool groups of the active boards and the Application group.
    pub(crate) fn groups(&self) -> &[ToolGroup] {
        self.catalog.as_ref().map_or(&[], GroupedCatalog::groups)
    }

    pub(crate) fn has_toasts(&self) -> bool {
        !self.toasts.is_empty()
    }

    pub(crate) fn take_toasts(&mut self) -> Vec<Toast> {
        std::mem::take(&mut self.toasts)
    }

    fn worker(&mut self) -> Result<&Worker, String> {
        if self.worker.is_none() {
            self.worker = Some(Worker::start()?);
        }
        self.worker
            .as_ref()
            .ok_or_else(|| "the MCP runtime is not available".to_owned())
    }

    fn stop_worker(&self) {
        if let Some(worker) = &self.worker {
            worker.stop();
        }
    }

    fn set_error(&mut self, reason: String) {
        self.toasts
            .push(Toast::error(format!("MCP server error: {reason}")));
        self.lifecycle = Lifecycle::Settled(ServerState::Error(reason));
    }
}

/// Installs the service and applies the saved settings (REQ-11, REQ-16).
pub(crate) fn init(service: McpService, cx: &mut App) {
    let settings = service.environment.config.mcp.clone();
    cx.set_global(service);
    cx.on_app_quit(stop_on_quit).detach();
    reconcile(&settings, cx);
}

/// Brings the app server in line with `settings` (REQ-06, REQ-12 to REQ-14).
pub(crate) fn reconcile(settings: &McpSettings, cx: &mut App) {
    let Some(service) = cx.try_global::<McpService>() else {
        return;
    };
    let token_file = token_file(cx);
    let target = target(settings, service);
    let decision = decide(
        &service.lifecycle,
        service.applied.as_ref(),
        &target,
        token_status(&token_file),
    );
    let catalog = GroupedCatalog::build(&service.boards, &[], &service.environment, settings);
    cx.global_mut::<McpService>().catalog = catalog.as_ref().ok().cloned();
    let plan = Plan {
        target,
        catalog,
        token_file,
        address: settings.address(),
    };
    apply(decision, plan, cx);
}

struct Plan {
    target: Target,
    catalog: Result<GroupedCatalog, String>,
    token_file: PathBuf,
    address: SocketAddr,
}

fn apply(decision: Decision, plan: Plan, cx: &mut App) {
    match decision {
        Decision::Noop => {}
        Decision::SwapCatalog => swap(plan, cx),
        Decision::Start | Decision::Restart => start(plan, cx),
        Decision::Stop => stop(cx),
        Decision::Refuse(status) => fail(token_reason(status).to_owned(), cx),
    }
}

fn target(settings: &McpSettings, service: &McpService) -> Target {
    Target {
        enabled: settings.enabled,
        listener: Listener {
            address: settings.address(),
            require_token: settings.require_token,
            token_revision: service.token_revision,
        },
        groups: settings.groups.clone(),
        boards: service.boards.iter().map(|board| board.id).collect(),
        repos: service.environment.config.repos.clone(),
    }
}

/// Serves the tools of `boards` with `config` after Settings enables or
/// disables a board or the favourite repositories change; an open listener
/// swaps its catalog.
pub(crate) fn set_boards(
    boards: &[crate::BoardRegistration],
    config: &std::sync::Arc<subspace_core::config::Config>,
    cx: &mut App,
) {
    if !cx.has_global::<McpService>() {
        return;
    }
    let service = cx.global_mut::<McpService>();
    service.boards = crate::registry::mcp_boards(boards);
    service.environment.config = config.clone();
    reconcile(&config.mcp, cx);
}

fn swap(plan: Plan, cx: &mut App) {
    match plan.catalog {
        Ok(catalog) => {
            let service = cx.global_mut::<McpService>();
            if let Some(worker) = &service.worker {
                worker.swap(catalog.served_catalog());
            }
            service.applied = Some(plan.target);
        }
        Err(error) => fail(error, cx),
    }
}

fn start(plan: Plan, cx: &mut App) {
    match host_config(&plan, cx).and_then(|config| send_start(config, plan, cx)) {
        Ok((reply, generation)) => await_start(reply, generation, cx),
        Err(error) => fail(error, cx),
    }
}

fn host_config(plan: &Plan, cx: &App) -> Result<HostConfig, String> {
    let token = if plan.target.listener.require_token {
        let secret = mcp_token::read(&plan.token_file)
            .map_err(|error| format!("cannot read the token file: {error}"))?;
        Some(secret.expose().to_owned())
    } else {
        None
    };
    let http = HttpOptions {
        bind: plan.address,
        token,
        origins: Vec::new(),
        hosts: Vec::new(),
    };
    Ok(HostConfig::new(http, identity(cx)))
}

fn send_start(config: HostConfig, plan: Plan, cx: &mut App) -> Result<(StartReply, u64), String> {
    let catalog = plan.catalog?.served_catalog();
    let service = cx.global_mut::<McpService>();
    let reply = service.worker()?.start_host(config, catalog);
    service.generation += 1;
    service.lifecycle = Lifecycle::Starting {
        generation: service.generation,
        shown: service.lifecycle.shown().clone(),
    };
    service.applied = Some(plan.target);
    Ok((reply, service.generation))
}

fn await_start(reply: StartReply, generation: u64, cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = reply
            .await
            .unwrap_or_else(|_| Err("the MCP runtime stopped".to_owned()));
        cx.update(|cx| finish_start(generation, result, cx));
    })
    .detach();
}

fn finish_start(generation: u64, result: Result<SocketAddr, String>, cx: &mut App) {
    let Some(service) = cx.try_global::<McpService>() else {
        return;
    };
    // A stop, a failure or a later start replaced this start.
    if !matches!(service.lifecycle, Lifecycle::Starting { generation: pending, .. } if pending == generation)
    {
        return;
    }
    let service = cx.global_mut::<McpService>();
    match result {
        Ok(address) => service.lifecycle = Lifecycle::Settled(ServerState::Listening(address)),
        Err(error) => service.set_error(error),
    }
}

fn stop(cx: &mut App) {
    let service = cx.global_mut::<McpService>();
    service.stop_worker();
    service.applied = None;
    service.lifecycle = Lifecycle::Settled(ServerState::Off);
}

fn fail(reason: String, cx: &mut App) {
    let service = cx.global_mut::<McpService>();
    service.stop_worker();
    service.applied = None;
    service.set_error(reason);
}

fn stop_on_quit(cx: &mut App) -> impl Future<Output = ()> + use<> {
    let stopped = cx
        .try_global::<McpService>()
        .and_then(|service| service.worker.as_ref())
        .map(Worker::stopped);
    async move {
        if let Some(stopped) = stopped {
            _ = stopped.await;
        }
    }
}

fn identity(cx: &App) -> Identity {
    let context = &cx.global::<CurrentProduct>().0;
    Identity::new(
        context.product.binary_name.as_str(),
        &context.product.display_name,
        &context.version,
    )
}

/// The token file beside the app's `config.json`.
pub(crate) fn token_file(cx: &App) -> PathBuf {
    token_path(&crate::product::gui_config_file(cx))
}

fn token_path(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .map_or_else(|| PathBuf::from("mcp-token"), mcp_token::path)
}

/// An unreadable status counts as not private, so the server stays stopped.
pub(crate) fn token_status(path: &Path) -> TokenStatus {
    mcp_token::status(path).unwrap_or(TokenStatus::NotPrivate)
}

/// Writes a new token (REQ-08). The next Save restarts a running server.
pub(crate) fn create_token(cx: &mut App) -> Result<(), String> {
    mcp_token::create_or_replace(&token_file(cx))
        .map_err(|error| format!("Cannot write the MCP token file: {error}"))?;
    if let Some(service) = cx.try_global::<McpService>() {
        let revision = service.token_revision + 1;
        cx.global_mut::<McpService>().token_revision = revision;
    }
    Ok(())
}

/// Queues a toast for the shell, which drains them as the service changes.
pub(crate) fn push_toast(toast: Toast, cx: &mut App) {
    if cx.has_global::<McpService>() {
        cx.global_mut::<McpService>().toasts.push(toast);
    }
}

pub(crate) fn state(cx: &App) -> ServerState {
    cx.try_global::<McpService>()
        .map_or(ServerState::Off, |service| {
            service.lifecycle.shown().clone()
        })
}

pub(crate) fn groups(cx: &App) -> Vec<ToolGroup> {
    cx.try_global::<McpService>()
        .map(|service| service.groups().to_vec())
        .unwrap_or_default()
}

/// The running app's tools for ACP chat sessions: every in-app tool of the
/// active boards, as Copilot and Pi chat get them, whatever Settings > MCP
/// serves to external clients. Each session gets its own loopback endpoint.
pub(crate) fn chat_tools(cx: &App) -> Option<Arc<dyn ChatToolServer>> {
    let service = cx.try_global::<McpService>()?;
    cx.try_global::<CurrentProduct>()?;
    let catalog = GroupedCatalog::build(
        &service.boards,
        &[],
        &service.environment,
        &McpSettings::default(),
    )
    .ok()?;
    Some(Arc::new(ChatTools::new(
        catalog.served_catalog(),
        identity(cx),
    )))
}

/// `http://<address>/mcp` while the app server listens.
pub(crate) fn listening_url(state: &ServerState) -> Option<String> {
    match state {
        ServerState::Listening(address) => {
            Some(format!("http://{address}{}", subspace_mcp::ENDPOINT))
        }
        ServerState::Off | ServerState::Error(_) => None,
    }
}

/// The page's status line (REQ-03).
pub(crate) fn status_text(state: &ServerState) -> String {
    match state {
        ServerState::Off => "Off".to_owned(),
        ServerState::Listening(_) => {
            format!("Listening on {}", listening_url(state).unwrap_or_default())
        }
        ServerState::Error(reason) => format!("Error: {reason}"),
    }
}

/// The agent prompt for the running app and its saved settings (REQ-29 to
/// REQ-31). It carries the token file path only, never its content.
pub(crate) fn agent_prompt(cx: &App) -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Cannot find this app's executable: {error}"))?;
    let require_token = cx
        .try_global::<Services>()
        .is_some_and(|services| services.config.mcp.require_token);
    let product = &cx.global::<CurrentProduct>().0.product;
    let url = listening_url(&state(cx));
    let token_file = token_file(cx);
    let mut input = PromptInput::new(
        product.binary_name.as_str(),
        &product.display_name,
        &executable,
    );
    input.listening_url = url.as_deref();
    input.token_file = require_token.then_some(token_file.as_path());
    Ok(subspace_mcp::agent_prompt(input))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ServerState, listening_url, status_text, token_path};

    #[test]
    fn the_status_line_names_the_state_and_url() {
        assert_eq!(status_text(&ServerState::Off), "Off");
        let listening = ServerState::Listening(([127, 0, 0, 1], 7331).into());
        assert_eq!(
            status_text(&listening),
            "Listening on http://127.0.0.1:7331/mcp"
        );
        let ipv6 = ServerState::Listening("[::1]:7400".parse().unwrap());
        assert_eq!(listening_url(&ipv6).unwrap(), "http://[::1]:7400/mcp");
        assert_eq!(
            status_text(&ServerState::Error("in use".into())),
            "Error: in use"
        );
        assert_eq!(listening_url(&ServerState::Off), None);
    }

    #[test]
    fn the_token_file_sits_beside_config_json() {
        assert_eq!(
            token_path(Path::new("/home/user/.config/subspace/config.json")),
            Path::new("/home/user/.config/subspace/mcp-token")
        );
    }
}
