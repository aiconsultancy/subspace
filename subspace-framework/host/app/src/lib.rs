//! Shared desktop shell. Product entrypoints route headless commands before
//! calling [`run`], then supply the selected dashboard factories and artwork.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, WindowOptions};
use subspace_agent_tools::{ToolEnvironment, ToolHost};
use subspace_core::config;
use subspace_core::credentials::Credentials;
use subspace_dashboard::dock_panel::DockPanelRegistration;
use subspace_dashboard::{Services, prefs};
use subspace_github_auth::OAuthClient;
use subspace_product::ProductContext;

mod account;
pub(crate) mod app;
mod assets;
mod build_metadata;
mod bundled_fonts;
#[cfg(any(test, all(debug_assertions, feature = "capture")))]
mod capture;
#[cfg(windows)]
mod console;
mod diagnostics;
#[cfg(debug_assertions)]
mod inspector;
mod jig;
mod logging;
mod mcp;
mod notifications;
mod palette;
mod product;
mod registry;
mod search_runtime;
mod settings;
mod startup;
mod startup_path;
mod status_bar;
mod terminal;
mod theme;
pub(crate) mod toolbar_labels;

pub use account::{AccountRegistration, AccountSession, AccountTokenSource, account_session};
pub use bundled_fonts::BundledFont;
pub use product::{config_file, prepare_context};
pub use registry::reload_configured_boards;
pub use registry::{BoardRegistration, ConfiguredBoards};
pub use search_runtime::RerankerFactory;
pub use settings::extensions::SettingsExtension;
pub use startup::{ProductBuild, launch};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

/// What the desktop app adds to the shell, from [`ProductBuild`].
pub(crate) struct DesktopOptions {
    pub account: Option<AccountRegistration>,
    pub display_name_prefix: Option<&'static str>,
    pub reranker: Option<RerankerFactory>,
    pub extensions: &'static [SettingsExtension],
    pub dock_panel: Option<DockPanelRegistration>,
    pub github_oauth: Option<OAuthClient>,
    pub fonts: &'static [BundledFont],
    #[cfg(feature = "report-issue")]
    pub report_issue_repository: Option<&'static str>,
}

/// Opens one product window. [`launch`] is the only caller.
fn run_desktop(
    context: ProductContext,
    boards: Vec<BoardRegistration>,
    configured: Option<ConfiguredBoards>,
    artwork: &'static [(&'static str, &'static [u8])],
    options: DesktopOptions,
) -> Result<(), String> {
    theme::validate_product_themes(&context.product)?;
    let files = product::ProductFiles::for_context(&context);
    let sign_in = options.github_oauth.is_some();
    let (mut environment, mut notices) =
        tool_environment(&files.config, &boards, ToolHost::App, sign_in);
    #[cfg(feature = "report-issue")]
    notices.extend(app::report_issue::target(options.report_issue_repository).err());
    notices.extend(logging::start(&files, &context));
    let (registrations, board_notices) =
        registry::desktop_sets(&context.product, &boards, configured, &environment.config)?;
    notices.extend(board_notices);
    if let Err(error) = search_runtime::initialize(
        &mut environment,
        &context,
        &registrations.active,
        &files,
        options.reranker,
    ) {
        notices.push(format!("Full-text search unavailable: {error}"));
    }
    let tools = registry::tool_catalog(&registrations.active, &environment)?;
    let mcp = mcp::McpService::new(&registrations.active, environment.clone());
    let context = Arc::new(context);
    #[cfg(windows)]
    console::detach()?;
    let application =
        gpui_kit::application().with_assets(assets::Assets::new(artwork, &registrations.included));
    application.on_reopen(ensure_product_window);
    application.run(move |cx| {
        gpui_kit::init(cx);
        product::install_context(context.clone(), options.display_name_prefix, cx);
        install_registrations(
            boards.clone(),
            configured,
            registrations,
            options.extensions,
            cx,
        );
        terminal::install(options.dock_panel, cx);
        prefs::init_file(Some(files.state.clone()), cx);
        let config_file = files.config.clone();
        cx.set_global(files);
        let notices = start_theme(notices, options.fonts, cx);
        install_services(environment, tools, config_file, options.github_oauth, cx);
        account::init(options.account, cx);
        #[cfg(feature = "report-issue")]
        app::report_issue::install(options.report_issue_repository, cx);
        bind_keys(cx);
        app::install_menus(cx);
        notifications::init(cx);
        search_runtime::start(cx);
        registry::sync_backgrounds(cx);
        mcp::init(mcp, cx);
        let name = product::desktop_name(cx).to_owned();
        open_product_window(notices, &name, cx);
    });
    Ok(())
}

/// Registers the app's fonts, then starts the theme, so the theme resolves
/// the families they add. The theme's notices follow `notices`.
fn start_theme(notices: Vec<String>, fonts: &'static [BundledFont], cx: &mut App) -> Vec<String> {
    notices.into_iter().chain(theme::start(fonts, cx)).collect()
}

/// The services every board and agent tool reads, and the one Sign in with
/// GitHub that uses their credentials, when the app has an OAuth App.
fn install_services(
    environment: ToolEnvironment,
    tools: subspace_dashboard::Catalog,
    config_file: std::path::PathBuf,
    github_oauth: Option<OAuthClient>,
    cx: &mut App,
) {
    cx.set_global(Services {
        config: environment.config,
        credentials: environment.credentials,
        tools,
        search: environment.search,
        config_file,
    });
    subspace_dashboard::github_sign_in::init(github_oauth, cx);
}

fn install_registrations(
    boards: Vec<BoardRegistration>,
    configured: Option<ConfiguredBoards>,
    registrations: registry::RegistrationSets,
    extensions: &'static [SettingsExtension],
    cx: &mut App,
) {
    registry::retain_provider(boards, configured, cx);
    notifications::install_defaults(&registrations.included, cx);
    cx.set_global(settings::extensions::SettingsExtensions(extensions));
    cx.set_global(registry::IncludedBoards(registrations.included));
    cx.set_global(registry::BoardFactories(registrations.active));
}

fn bind_keys(cx: &mut App) {
    app::bind_keys(cx);
    terminal::bind_keys(cx);
    subspace_dashboard::bind_keys(cx);
    #[cfg(debug_assertions)]
    inspector::init(cx);
}

/// The product's configuration, with the own settings files of `boards`, and
/// credentials, as every board and agent tool in `host` receives them, with
/// the configuration notices as text. `sign_in` is whether the app offers
/// Sign in with GitHub.
pub(crate) fn tool_environment(
    config_file: &std::path::Path,
    boards: &[BoardRegistration],
    host: ToolHost,
    sign_in: bool,
) -> (ToolEnvironment, Vec<String>) {
    let (config, notices) =
        config::load_with_boards(config_file, boards.iter().map(|board| board.id));
    let credentials = Arc::new(Credentials::from_system(&config).with_sign_in_offered(sign_in));
    let environment = ToolEnvironment {
        config: Arc::new(config),
        credentials,
        search: None,
        config_file: Some(config_file.to_owned()),
        host,
    };
    (
        environment,
        notices.iter().map(ToString::to_string).collect(),
    )
}

fn open_product_window(notices: Vec<String>, name: &str, cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(app::window_bounds(cx)),
        titlebar: Some(app::titlebar::options(name.to_owned())),
        app_owns_titlebar_drag: app::titlebar::INTEGRATED,
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| app::SubspaceApp::new(notices, window, cx))
    });
    if let Err(error) = opened {
        let message = format!("{name} could not open its window: {error}");
        subspace_core::log::Logger::new("app").record(
            subspace_core::log::Level::Error,
            &message,
            &[],
        );
        eprintln!("{message}");
        cx.quit();
    }
    #[cfg(all(debug_assertions, feature = "capture"))]
    capture::start(cx);
}

/// Only explicit activation (Dock or notification click) creates a replacement window.
fn ensure_product_window(cx: &mut App) {
    let Some(runtime) = cx.try_global::<notifications::Runtime>() else {
        return;
    };
    if let Some((window, app)) = runtime.window.clone()
        && let Some(app) = app.upgrade()
        && window
            .update(cx, |_, window, cx| {
                let pending = cx.global_mut::<notifications::Runtime>().pending.take();
                if let Some(route) = pending {
                    app.update(cx, |app, cx| app.open_notification(route, window, cx));
                }
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    let name = product::desktop_name(cx).to_owned();
    open_product_window(Vec::new(), &name, cx);
}
