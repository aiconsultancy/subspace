//! One product entrypoint. Headless routes run before desktop initialization.

use std::process::ExitCode;

use subspace_product::{
    BuildInfo, Channel, DataNameError, HostCommand, ProductContext, ProductDescriptor,
    host_command, product_metadata, validate_product,
};

use crate::{BoardRegistration, DesktopOptions, prepare_context, run_desktop};

mod headless;

use headless::headless;

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
    /// Runs after startup `PATH` setup, before configuration, theme validation
    /// or GPUI. Arguments exclude the binary name. `Some(code)` exits;
    /// `None` continues normal startup. No capability gate applies.
    pub pre_startup: Option<fn(&[String]) -> Option<ExitCode>>,
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
    /// Font files the app embeds, registered with the text system before the
    /// theme is applied. A theme or Settings that names one of their families
    /// gets it on every platform; automatic choices still use the platform's
    /// fonts. `&[]` embeds none.
    pub fonts: &'static [crate::BundledFont],
    /// Theme documents the product embeds beside the Framework's catalogue,
    /// as `(id, json)`. List an id in the descriptor's `themes`, and name one
    /// of its variants as `default_theme`, to use it. An id must not repeat a
    /// Framework theme id. `&[]` adds none.
    pub product_themes: crate::ProductThemes,
    /// The GitHub repository (`owner/name`) Report issue files into. `None`
    /// or blank hides Report issue; a malformed one hides it with a notice.
    #[cfg(feature = "report-issue")]
    pub report_issue_repository: Option<&'static str>,
}

/// Whether `args` start a route that reads the app's configuration. The
/// product metadata, About, `--version` and `upgrade` read none, so an app
/// that checks its own settings before [`launch`] skips those checks for
/// them, and a broken setting never blocks an upgrade.
// qual:api - a product calls it before its own start-up checks.
pub fn reads_configuration(descriptor: &ProductDescriptor, args: &[String]) -> bool {
    reads_configuration_for(args, descriptor.update.is_some())
}

fn reads_configuration_for(args: &[String], updates: bool) -> bool {
    !matches!(
        route(args, updates),
        StartupRoute::Metadata
            | StartupRoute::About
            | StartupRoute::Version
            | StartupRoute::Upgrade
    )
}

impl ProductBuild {
    /// Build a product with no optional host integrations or startup hook.
    pub fn new(descriptor: ProductDescriptor, version: &'static str, build: BuildInfo) -> Self {
        Self {
            descriptor,
            version,
            build,
            pre_startup: None,
            account: None,
            display_name_prefix: None,
            search_reranker: None,
            default_theme_override: None,
            configured_boards: None,
            settings_extensions: &[],
            dock_panel: None,
            github_oauth: None,
            fonts: &[],
            product_themes: &[],
            #[cfg(feature = "report-issue")]
            report_issue_repository: None,
        }
    }
}

type CommandRunner = fn(
    &ProductContext,
    &[subspace_mcp::Board],
    &subspace_agent_tools::ToolEnvironment,
    &[String],
) -> ExitCode;

/// `updates` is whether this app has an update source. Without one,
/// `upgrade` is not a Framework command and falls through to the normal
/// command path, which reports it as unknown.
fn route(args: &[String], updates: bool) -> StartupRoute {
    startup_route(args.is_empty(), host_command(args, updates))
}

fn startup_route(empty: bool, host: Option<HostCommand>) -> StartupRoute {
    match (empty, host) {
        (true, _) => StartupRoute::Desktop,
        (_, Some(HostCommand::Metadata)) => StartupRoute::Metadata,
        (_, Some(HostCommand::About)) => StartupRoute::About,
        (_, Some(HostCommand::Version)) => StartupRoute::Version,
        (_, Some(HostCommand::Mcp)) => StartupRoute::Mcp,
        (_, Some(HostCommand::Upgrade)) => StartupRoute::Upgrade,
        (false, None) => StartupRoute::Command,
    }
}

/// Called by thin binaries with only their selected board providers. Every
/// mode first names the data directory after the binary, unless the app
/// called [`subspace_product::configure_data_name`] first, then gets the
/// user's `PATH`; nothing before that starts a thread. It fails when
/// something read a data path under another name before it.
pub fn launch(
    build: ProductBuild,
    boards: Vec<BoardRegistration>,
    artwork: &'static [(&'static str, &'static [u8])],
    command: CommandRunner,
) -> ExitCode {
    if let Err(error) = name_data_directory(build.descriptor.binary_name.as_str()) {
        return finish(Err(error));
    }
    crate::startup_path::apply();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    launch_after_path(build, boards, artwork, command, &args)
}

fn launch_after_path(
    build: ProductBuild,
    boards: Vec<BoardRegistration>,
    artwork: &'static [(&'static str, &'static [u8])],
    command: CommandRunner,
    args: &[String],
) -> ExitCode {
    if let Some(code) = build.pre_startup.and_then(|hook| hook(args)) {
        return code;
    }
    prepare_startup(build, (boards, artwork), command, args)
}

fn prepare_startup(
    build: ProductBuild,
    (boards, artwork): (
        Vec<BoardRegistration>,
        &'static [(&'static str, &'static [u8])],
    ),
    command: CommandRunner,
    args: &[String],
) -> ExitCode {
    let mut build = build;
    let descriptor = &mut build.descriptor;
    let version = build.version;
    if let Err(error) = descriptor.apply_default_theme_override(build.default_theme_override) {
        return finish(Err(error.to_string()));
    }
    subspace_update::cleanup_previous(&descriptor.binary_name);
    let updates = descriptor.update.is_some();
    warn_prerelease(descriptor, version, args, &route(args, updates));
    if route(args, updates) == StartupRoute::Metadata {
        return finish(print_metadata(descriptor, version));
    }
    if let Some(text) = build_text(&route(args, updates), descriptor, version, &build.build) {
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    if route(args, updates) == StartupRoute::Upgrade {
        return upgrade(descriptor, version, args);
    }
    let context = match prepare_context(build.descriptor.clone(), version) {
        Ok(context) => ProductContext {
            build: build.build,
            ..context
        },
        Err(error) => return finish(Err(error)),
    };
    start_mode(build, context, (boards, artwork), args, command)
}

fn start_mode(
    build: ProductBuild,
    context: ProductContext,
    (boards, artwork): (
        Vec<BoardRegistration>,
        &'static [(&'static str, &'static [u8])],
    ),
    args: &[String],
    command: CommandRunner,
) -> ExitCode {
    match route(args, context.product.update.is_some()) {
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
                fonts: build.fonts,
                product_themes: build.product_themes,
                #[cfg(feature = "report-issue")]
                report_issue_repository: build.report_issue_repository,
            },
        )),
        _ => {
            let sign_in = (boards.as_slice(), build.github_oauth.is_some());
            headless(&context, sign_in, args, build.search_reranker, command)
        }
    }
}

/// Names the data directory after the binary, unless the app named it
/// first; `launch`, `prepare_context` and `record_new_install_theme` call
/// it. Fails while a read has implicitly fixed another name, so the
/// process never uses two data directories.
pub(crate) fn name_data_directory(binary_name: &str) -> Result<(), String> {
    data_name_outcome(subspace_product::configure_data_name(binary_name))
}

fn data_name_outcome(result: Result<(), DataNameError>) -> Result<(), String> {
    match result {
        Ok(()) | Err(DataNameError::Configured(_)) => Ok(()),
        Err(error) => Err(error.to_string()),
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
    use subspace_product::{Channel, DataNameError, ProductError};

    use super::{StartupRoute, data_name_outcome, route, warning_line, warns};

    #[cfg(unix)]
    mod former_ids;

    #[test]
    fn launch_keeps_an_app_s_own_data_name_and_refuses_a_second_one() {
        assert_eq!(data_name_outcome(Ok(())), Ok(()));
        assert_eq!(
            data_name_outcome(Err(DataNameError::Configured("example-app".into()))),
            Ok(())
        );
        let read_first = data_name_outcome(Err(DataNameError::Resolved("subspace".into())));
        assert!(
            read_first.as_ref().is_err_and(
                |error| error.contains("'subspace'") && error.contains("configure_data_name")
            ),
            "{read_first:?}"
        );
        let invalid = DataNameError::Invalid(ProductError::UnsafeIdentity { name: "..".into() });
        assert!(data_name_outcome(Err(invalid)).is_err());
    }

    mod pre_startup;

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
    fn a_lone_about_is_start_ups_but_a_space_named_about_keeps_its_commands() {
        assert_eq!(route(&words(&["about"]), true), StartupRoute::About);
        assert_eq!(
            route(&words(&["about", "read"]), true),
            StartupRoute::Command
        );
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
    fn only_desktop_mcp_and_commands_read_the_configuration() {
        let updating = |args: &[&str]| super::reads_configuration_for(&words(args), true);
        for args in [
            &["upgrade"][..],
            &["--version"],
            &["about"],
            &["__product_metadata"],
        ] {
            assert!(!updating(args), "{args:?}");
        }
        for args in [&[][..], &["mcp"], &["issues", "list"]] {
            assert!(updating(args), "{args:?}");
        }
        assert!(super::reads_configuration_for(&words(&["upgrade"]), false));
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
