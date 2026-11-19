//! One product entrypoint. Headless routes run before desktop initialization.

use std::process::ExitCode;

use subspace_product::{
    BuildInfo, Channel, MCP_COMMAND, ProductContext, ProductDescriptor, UPGRADE_COMMAND,
    product_metadata, validate_product,
};

use crate::{BoardRegistration, DesktopOptions, prepare_context, run_desktop};

#[derive(Debug, PartialEq, Eq)]
enum StartupRoute {
    Desktop,
    Metadata,
    About,
    Version,
    Mcp,
    Upgrade,
    Command,
}

/// Compile-time identity supplied by a thin product binary.
pub struct ProductBuild {
    /// The app's native account: its identity provider (Entra, WorkOS or
    /// its own) and the trusted token consumer it installs before Spaces
    /// start. `None` shows no Account settings page.
    pub account: Option<crate::AccountRegistration>,
    pub descriptor: ProductDescriptor,
    /// Optional prefix for desktop window titles, menus and About.
    pub display_name_prefix: Option<&'static str>,
    pub version: &'static str,
    /// How this binary was built, stamped by the product's build script.
    pub build: BuildInfo,
    pub search_reranker: Option<crate::RerankerFactory>,
    pub default_theme_override: Option<&'static str>,
    /// Boards the desktop app adds from its config file. Headless commands
    /// do not use them.
    pub configured_boards: Option<crate::ConfiguredBoards>,
    pub settings_extensions: &'static [crate::SettingsExtension],
    /// The panel the shell docks beside every Space, such as the terminal.
    /// `None` shows no terminal button, action, menu item or command.
    pub dock_panel: Option<subspace_dashboard::dock_panel::DockPanelRegistration>,
    /// The app's GitHub OAuth App, for Sign in with GitHub. The Framework has
    /// none: `None` offers no Sign in with GitHub anywhere, and tokens come
    /// from the environment, the config file and the `gh` CLI as before.
    pub github_oauth: Option<subspace_github_auth::OAuthClient>,
    /// The GitHub repository (`owner/name`) Report issue files into. `None`
    /// or blank hides Report issue; a malformed one hides it with a notice.
    #[cfg(feature = "report-issue")]
    pub report_issue_repository: Option<&'static str>,
}

/// `updates` is whether this app has an update source. Without one,
/// `upgrade` is not a Framework command and falls through to the normal
/// command path, which reports it as unknown.
fn route(args: &[String], updates: bool) -> StartupRoute {
    match args {
        [] => StartupRoute::Desktop,
        [arg] if arg == "__product_metadata" => StartupRoute::Metadata,
        [arg] if arg == "about" || arg == "--about" => StartupRoute::About,
        [arg] if arg == "--version" => StartupRoute::Version,
        [first, ..] if first == MCP_COMMAND => StartupRoute::Mcp,
        [first, ..] if updates && first == UPGRADE_COMMAND => StartupRoute::Upgrade,
        _ => StartupRoute::Command,
    }
}

/// Called by thin binaries with only their selected board providers. Every
/// mode first gets the user's `PATH`; nothing before that starts a thread.
pub fn launch(
    build: ProductBuild,
    boards: Vec<BoardRegistration>,
    artwork: &'static [(&'static str, &'static [u8])],
    command: fn(
        &ProductContext,
        &[subspace_mcp::Board],
        &subspace_agent_tools::ToolEnvironment,
        &[String],
    ) -> ExitCode,
) -> ExitCode {
    crate::startup_path::apply(&build.descriptor);
    let mut descriptor = build.descriptor;
    let version = build.version;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = descriptor.apply_default_theme_override(build.default_theme_override) {
        return finish(Err(error.to_string()));
    }
    subspace_update::cleanup_previous(&descriptor.binary_name);
    let updates = descriptor.update.is_some();
    warn_prerelease(&descriptor, version, &args, &route(&args, updates));
    if route(&args, updates) == StartupRoute::Metadata {
        return finish(print_metadata(&descriptor, version));
    }
    if let Some(text) = build_text(&route(&args, updates), &descriptor, version, &build.build) {
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    if route(&args, updates) == StartupRoute::Upgrade {
        return upgrade(&descriptor, version, &args);
    }
    let context = match prepare_context(descriptor, version) {
        Ok(context) => ProductContext {
            build: build.build,
            ..context
        },
        Err(error) => return finish(Err(error)),
    };
    match route(&args, updates) {
        StartupRoute::Desktop => finish(run_desktop(
            context,
            boards,
            build.configured_boards,
            artwork,
            DesktopOptions {
                account: build.account,
                display_name_prefix: build.display_name_prefix,
                reranker: build.search_reranker,
                extensions: build.settings_extensions,
                dock_panel: build.dock_panel,
                github_oauth: build.github_oauth,
                #[cfg(feature = "report-issue")]
                report_issue_repository: build.report_issue_repository,
            },
        )),
        _ => {
            let sign_in = (boards.as_slice(), build.github_oauth.is_some());
            headless(&context, sign_in, &args, build.search_reranker, command)
        }
    }
}

/// What `about` and `--version` print; other routes print nothing here.
fn build_text(
    route: &StartupRoute,
    descriptor: &ProductDescriptor,
    version: &str,
    build: &BuildInfo,
) -> Option<String> {
    match route {
        StartupRoute::About => Some(crate::build_metadata::about_text(
            descriptor, version, build,
        )),
        StartupRoute::Version => Some(crate::build_metadata::version_text(
            descriptor, version, build,
        )),
        _ => None,
    }
}

/// Environment variable that hides the pre-release warning when set to `1`.
const NO_PRERELEASE_WARNING: &str = "SUBSPACE_NO_PRERELEASE_WARNING";

/// Writes the one-line pre-release warning to stderr before a CLI command.
fn warn_prerelease(
    descriptor: &ProductDescriptor,
    version: &str,
    args: &[String],
    route: &StartupRoute,
) {
    let suppressed = std::env::var(NO_PRERELEASE_WARNING).is_ok_and(|value| value == "1");
    if warns(Channel::current(), route, args, suppressed) {
        eprintln!("{}", warning_line(&descriptor.display_name, version));
    }
}

/// True for a pre-release build running a CLI command whose output is not
/// machine-read: never for the desktop app, `--version`,
/// `__product_metadata` or a `--json` command.
fn warns(channel: Channel, route: &StartupRoute, args: &[String], suppressed: bool) -> bool {
    let machine_output = args
        .iter()
        .any(|arg| arg == "--json" || arg.starts_with("--json="));
    let cli = !matches!(
        route,
        StartupRoute::Desktop | StartupRoute::Metadata | StartupRoute::Version
    );
    channel.is_prerelease() && cli && !machine_output && !suppressed
}

fn warning_line(display_name: &str, version: &str) -> String {
    format!("warning: {display_name} {version} is a pre-release build")
}

/// `upgrade`: the app's own release check and install, before any
/// configuration is read, so a broken configuration cannot block an update.
fn upgrade(descriptor: &ProductDescriptor, version: &str, args: &[String]) -> ExitCode {
    match validate_product(descriptor) {
        Ok(product) => subspace_update::run_cli(&product, version, args),
        Err(error) => finish(Err(error.to_string())),
    }
}

/// `mcp`: the selected boards' agent tools for other harnesses, without GPUI
/// or a Copilot login.
/// `sign_in` is whether the app offers Sign in with GitHub, which guidance
/// may then name.
fn headless(
    context: &ProductContext,
    (boards, sign_in): (&[BoardRegistration], bool),
    args: &[String],
    reranker: Option<crate::RerankerFactory>,
    command: fn(
        &ProductContext,
        &[subspace_mcp::Board],
        &subspace_agent_tools::ToolEnvironment,
        &[String],
    ) -> ExitCode,
) -> ExitCode {
    let files = crate::product::ProductFiles::for_context(context);
    let (mut environment, notices) = crate::tool_environment(
        &files.config,
        boards,
        subspace_agent_tools::ToolHost::Headless,
        sign_in,
    );
    let log_notice = crate::logging::start(&files, context);
    for notice in notices.into_iter().chain(log_notice) {
        eprintln!("{notice}");
    }
    let selected = match crate::registry::selected_registrations(
        &context.product,
        boards,
        &environment.config,
    ) {
        Ok(selected) => selected,
        Err(error) => return finish(Err(error)),
    };
    if let Err(error) =
        crate::search_runtime::initialize(&mut environment, context, &selected, &files, reranker)
    {
        eprintln!("Full-text search unavailable: {error}");
    }
    let boards = crate::registry::mcp_boards(&selected);
    if args.first().is_some_and(|arg| arg == "mcp") {
        subspace_mcp::run(
            context,
            &boards,
            &environment,
            &args[1..],
            &environment.config.mcp,
        )
    } else {
        command(context, &boards, &environment, args)
    }
}

fn print_metadata(descriptor: &ProductDescriptor, version: &str) -> Result<(), String> {
    let metadata = product_metadata(descriptor, version).map_err(|error| error.to_string())?;
    println!("{metadata}");
    Ok(())
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use subspace_product::Channel;

    use super::{StartupRoute, route, warning_line, warns};

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn nightly_warns(args: &[&str], suppressed: bool) -> bool {
        let args = words(args);
        warns(Channel::Nightly, &route(&args, true), &args, suppressed)
    }

    #[test]
    fn a_pre_release_cli_command_warns_once_with_its_name_and_version() {
        assert_eq!(
            warning_line("Subspace (Nightly)", "1.10.2-nightly.202611010200.ga43a333"),
            "warning: Subspace (Nightly) 1.10.2-nightly.202611010200.ga43a333 is a pre-release build"
        );
        for args in [
            &["about"][..],
            &["upgrade", "--check"],
            &["mcp"],
            &["github", "read"],
        ] {
            assert!(nightly_warns(args, false), "{args:?}");
        }
    }

    #[test]
    fn machine_output_the_desktop_suppression_and_stable_never_warn() {
        for args in [
            &[][..],
            &["--version"],
            &["__product_metadata"],
            &["github", "read", "--json"],
            &["github", "read", "--json=true"],
        ] {
            assert!(!nightly_warns(args, false), "{args:?}");
        }
        assert!(!nightly_warns(&["about"], true));
        let about = words(&["about"]);
        assert!(!warns(Channel::Stable, &route(&about, true), &about, false));
    }

    #[test]
    fn only_no_arguments_choose_desktop_initialization() {
        assert_eq!(route(&[], true), StartupRoute::Desktop);
        for args in [["--help"], ["--version"], ["tui"], ["unknown"]] {
            let expected = if args == ["--version"] {
                StartupRoute::Version
            } else {
                StartupRoute::Command
            };
            assert_eq!(route(&args.map(str::to_owned), true), expected);
        }
        assert_eq!(
            route(&["__product_metadata".into()], true),
            StartupRoute::Metadata
        );
        assert_eq!(route(&["about".into()], true), StartupRoute::About);
        assert_eq!(route(&["--about".into()], true), StartupRoute::About);
    }

    #[test]
    fn mcp_is_routed_before_the_board_commands() {
        assert_eq!(route(&["mcp".into()], true), StartupRoute::Mcp);
        assert_eq!(
            route(&["mcp".into(), "--http".into()], true),
            StartupRoute::Mcp
        );
        assert_eq!(
            route(&["github".into(), "mcp".into()], true),
            StartupRoute::Command
        );
    }

    #[test]
    fn upgrade_is_routed_before_any_configuration_when_the_app_has_updates() {
        for args in [
            vec!["upgrade".to_owned()],
            vec!["upgrade".to_owned(), "--check".to_owned()],
            vec!["upgrade".to_owned(), "--version".to_owned(), "1.2.0".into()],
        ] {
            assert_eq!(route(&args, true), StartupRoute::Upgrade);
        }
        assert_eq!(
            route(&["github".into(), "upgrade".into()], true),
            StartupRoute::Command
        );
    }

    #[test]
    fn upgrade_falls_through_without_an_update_source() {
        assert_eq!(route(&["upgrade".into()], false), StartupRoute::Command);
        assert_eq!(
            route(&["upgrade".into(), "--check".into()], false),
            StartupRoute::Command
        );
        assert_eq!(route(&["mcp".into()], false), StartupRoute::Mcp);
    }
}
