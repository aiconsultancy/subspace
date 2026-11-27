use std::process::{Command, ExitCode};

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, Window, div,
};
use subspace_agent_tools::ToolEnvironment;
use subspace_dashboard::{Dashboard, DashboardEvent};
use subspace_product::{BuildInfo, ProductContext, fixture};

use super::super::{ProductBuild, launch_after_path};
use crate::BoardRegistration;

const CHILD: &str = "EXAMPLE_ALIAS_CHILD";
const TEST: &str = "startup::tests::former_ids::a_start_reads_former_ids_without_writing";

/// The one Space of the fixture app, renamed from `example-old`.
struct Renamed {
    focus: FocusHandle,
}
impl EventEmitter<DashboardEvent> for Renamed {}
impl Focusable for Renamed {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Renamed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl Dashboard for Renamed {
    const ID: &'static str = "example-new";
    const TITLE: &'static str = "Renamed";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

/// An app whose one Space, `example-new`, was `example-old`.
fn build() -> ProductBuild {
    let mut descriptor = fixture::descriptor();
    descriptor.id = "example-app".into();
    descriptor.binary_name = "example-app".into();
    descriptor.boards.truncate(1);
    descriptor.boards[0].id = Renamed::ID.into();
    descriptor.boards[0].former_ids = vec!["example-old".into()];
    descriptor.update = None;
    ProductBuild::new(descriptor, "1.0.0", BuildInfo::UNSTAMPED)
}

/// The config a person had before the rename. `repos` is a warning, which
/// the Settings writer refuses; the readers still map the former id.
const CONFIG: &str = r#"{"boards":{"example-old":false,"example-short":false},
    "boardOrder":["example-old"],"repos":"not a list",
    "mcp":{"groups":{"example-old":false}},
    "commands":{"aliases":{"example-old":["example-gone"],"example-new":["example-short"]}}}"#;

const STATE: &str = r#"{"example-old.size":"4","example-short.size":"2"}"#;

/// The board command: the start-up has loaded the config and read it as the
/// Space's own, written nothing, and taken the Space's own shortcut list.
/// The person's `example-short` shortcut is no former id.
fn command(
    context: &ProductContext,
    _: &[subspace_mcp::Board],
    environment: &ToolEnvironment,
    _: &[String],
) -> ExitCode {
    let files = crate::product::ProductFiles::for_context(context);
    assert!(!environment.config.board_enabled(Renamed::ID));
    assert!(!environment.config.mcp.group_enabled(Renamed::ID));
    assert_eq!(environment.config.board_order, [Renamed::ID]);
    let space = context.product.board_for_command("example-short").unwrap();
    assert_eq!(space.id.as_str(), Renamed::ID);
    assert!(context.product.board_for_command("example-gone").is_none());
    let state = subspace_core::state::State::load(&files.state);
    assert_eq!(state.get("example-new.size"), Some("4"));
    assert_eq!(state.get("example-short.size"), Some("2"));
    assert_eq!(std::fs::read_to_string(&files.config).unwrap(), CONFIG);
    assert_eq!(std::fs::read_to_string(&files.state).unwrap(), STATE);
    ExitCode::SUCCESS
}

/// Writes the files a person had before the rename, then starts the app as
/// `launch` does, with a board command.
fn child() {
    let build = build();
    let context = crate::prepare_context(build.descriptor.clone(), build.version).unwrap();
    let files = crate::product::ProductFiles::for_context(&context);
    std::fs::create_dir_all(files.config.parent().unwrap()).unwrap();
    std::fs::create_dir_all(files.state.parent().unwrap()).unwrap();
    std::fs::write(&files.config, CONFIG).unwrap();
    std::fs::write(&files.state, STATE).unwrap();
    let boards = vec![BoardRegistration::new::<Renamed>()];
    let args = ["example-new".to_owned(), "check".to_owned()];
    assert_eq!(
        launch_after_path(build, boards, &[], command, &args),
        ExitCode::SUCCESS
    );
}

#[test]
fn a_start_reads_former_ids_without_writing() {
    if std::env::var_os(CHILD).is_some() {
        return child();
    }
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, "1")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_STATE_HOME", home.path().join(".local/state"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("running 1 test"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
