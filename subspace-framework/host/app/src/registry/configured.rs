//! Boards an app builds from its config file, after the boards its
//! descriptor lists.

use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::{App, Global, Window};
use subspace_agent_tools::{ToolEnvironment, ToolHost};
use subspace_core::config::Config;
use subspace_dashboard::settings::BoardSettings;
use subspace_dashboard::{Dashboard, DashboardHandle, Services};
use subspace_product::{BoardIdUse, ValidatedProduct};

use super::{BoardRegistration, RegistrationSets, registration_sets_with};

/// The boards an app builds from its config file, such as `webBoards`.
/// Apps without one show only the boards their descriptor lists.
pub type ConfiguredBoards = fn(&Config) -> Vec<BoardRegistration>;

pub(crate) struct Provider {
    available: Vec<BoardRegistration>,
    configured: Option<ConfiguredBoards>,
}
impl Global for Provider {}

pub(crate) fn retain_provider(
    available: Vec<BoardRegistration>,
    configured: Option<ConfiguredBoards>,
    cx: &mut App,
) {
    cx.set_global(Provider {
        available,
        configured,
    });
}

/// Drops the retained provider, so the board set cannot be rebuilt and a
/// board choice applies only after a restart.
#[cfg(test)]
pub(crate) fn forget_provider(cx: &mut App) {
    cx.remove_global::<Provider>();
}

/// Reloads the configured provider after a checked, confirmed config write.
/// Shells observe the factory set and reconcile their retained views and editors.
/// Native boards' own files are not part of that write, so their settings stay.
// qual:api
pub fn reload_configured_boards(window: &mut Window, cx: &mut App) -> Result<(), String> {
    let services = cx.global::<Services>();
    let path = services.config_file.clone();
    let board_settings = services.config.board_settings.clone();
    subspace_core::settings::SettingsFile::open(&path).map_err(|error| error.to_string())?;
    let (mut config, _) = subspace_core::config::load(&path);
    config.board_settings = board_settings;
    apply_board_selection(config, window, cx)
}

/// Rebuilds the agent tools and the MCP catalog from the config the app runs
/// with now, after a board changed it (`DashboardEvent::ConfigChanged`), such
/// as the favourite repositories.
pub(crate) fn apply_running_config(window: &mut Window, cx: &mut App) -> Result<(), String> {
    let config = (*cx.global::<Services>().config).clone();
    apply_board_selection(config, window, cx)
}

/// Applies the boards `config` enables without a restart: shells add and
/// remove tabs, background work starts and stops, and chat and MCP serve
/// the tools of the active boards. Unchanged registrations keep their views.
pub(crate) fn apply_board_selection(
    config: Config,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let mut sets = selected_sets(&config, cx)?;
    let previous = super::included_registrations(cx);
    reuse_registrations(&mut sets.included, &previous);
    reuse_registrations(&mut sets.active, &previous);
    install_sets(config, sets, cx)?;
    window.refresh();
    Ok(())
}

/// Whether applying `config` would leave at least one board active, as
/// [`apply_board_selection`] or the next start builds the set. Boards the
/// config file defines but cannot be shown do not count.
pub(crate) fn enables_a_board(config: &Config, cx: &App) -> Result<bool, String> {
    let provider = cx
        .try_global::<Provider>()
        .ok_or("No configured board provider is retained.")?;
    let product = &cx.global::<subspace_dashboard::CurrentProduct>().0.product;
    let (sets, _) = desktop_sets(product, &provider.available, provider.configured, config)?;
    Ok(!sets.active.is_empty())
}

fn selected_sets(config: &Config, cx: &App) -> Result<RegistrationSets, String> {
    let provider = cx
        .try_global::<Provider>()
        .ok_or("No configured board provider is retained.")?;
    let product = &cx.global::<subspace_dashboard::CurrentProduct>().0.product;
    let (sets, notices) = desktop_sets(product, &provider.available, provider.configured, config)?;
    if notices.is_empty() {
        Ok(sets)
    } else {
        Err(notices.join("\n"))
    }
}

fn install_sets(config: Config, sets: RegistrationSets, cx: &mut App) -> Result<(), String> {
    let config = Arc::new(config);
    let environment = tool_environment(config.clone(), cx.global::<Services>());
    let tools = super::tool_catalog(&sets.active, &environment)?;
    let services = cx.global_mut::<Services>();
    services.config = config.clone();
    services.tools = tools;
    crate::mcp::set_boards(&sets.active, &config, cx);
    crate::notifications::install_defaults(&sets.included, cx);
    cx.set_global(super::IncludedBoards(sets.included));
    cx.set_global(super::BoardFactories(sets.active));
    super::sync_backgrounds(cx);
    Ok(())
}

/// The environment chat tools receive, with `config` as the app now runs it.
fn tool_environment(config: Arc<Config>, services: &Services) -> ToolEnvironment {
    ToolEnvironment {
        config,
        credentials: Arc::clone(&services.credentials),
        search: services.search.clone(),
        config_file: Some(services.config_file.clone()),
        host: ToolHost::App,
    }
}

fn reuse_registrations(next: &mut [BoardRegistration], previous: &[BoardRegistration]) {
    for board in next {
        if let Some(old) = previous.iter().find(|old| old.id == board.id)
            && configuration_matches(old, board)
            && old.title == board.title
            && old.description == board.description
            && old.settings == board.settings
            && old.icon == board.icon
        {
            *board = old.clone();
        }
    }
}

fn configuration_matches(old: &BoardRegistration, next: &BoardRegistration) -> bool {
    match (&old.configuration, &next.configuration) {
        (Some(old), Some(next)) => old == next,
        (None, None) => Arc::ptr_eq(&old.create, &next.create),
        _ => false,
    }
}

impl BoardRegistration {
    /// One instance of board type `D` that the config file defines, such as
    /// a web board. Instances have no agent tools, search, features or
    /// background work of their own, so several can share one type.
    pub fn configured<D: Dashboard>(
        id: &str,
        title: &str,
        description: &str,
        create: impl Fn(&mut Window, &mut App) -> DashboardHandle + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: intern_id(id),
            title: title.to_owned().into(),
            description: description.to_owned().into(),
            icon: D::ICON,
            help: D::HELP_MARKDOWN,
            help_images: D::HELP_IMAGES,
            searchable: false,
            create: Arc::new(create),
            tools: |_| Vec::new(),
            settings: BoardSettings {
                sections: &[],
                features: &[],
                ..D::settings()
            },
            start_background: |_| {},
            stop_background: |_| {},
            retain_when_disabled: D::RETAIN_WHEN_DISABLED,
            fields: None,
            settings_editor: D::settings_editor(),
            configuration: None,
            default_order: usize::MAX,
            artwork: D::ARTWORK,
            connection: D::connection_settings(),
            // Every instance declares its type's roles under its own ID. A
            // configured type must not respond: two instances would be two
            // responders, which registration rejects.
            contracts: D::contracts(),
        }
    }
}

/// The desktop's boards: the product's own and the app's configured boards,
/// with a notice for each configured board left out.
pub(crate) fn desktop_sets(
    product: &ValidatedProduct,
    available: &[BoardRegistration],
    configured: Option<ConfiguredBoards>,
    config: &Config,
) -> Result<(RegistrationSets, Vec<String>), String> {
    let configured = configured.map(|build| build(config)).unwrap_or_default();
    let (configured, notices) = accept_configured(product, configured);
    let sets = registration_sets_with(product, available, configured, config)?;
    Ok((sets, notices))
}

/// The configured boards whose ids the product does not already use, as a
/// Space's id or former id, with a notice for each one left out.
pub(super) fn accept_configured(
    product: &ValidatedProduct,
    configured: Vec<BoardRegistration>,
) -> (Vec<BoardRegistration>, Vec<String>) {
    let mut notices = Vec::new();
    let accepted = configured
        .into_iter()
        .filter(|board| match built_in_use(product, board.id) {
            Some(reason) => {
                notices.push(format!("Configured board {} ignored: {reason}", board.id));
                false
            }
            None => true,
        })
        .collect();
    (accepted, notices)
}

/// Why the product's own Spaces leave no room for a configured board `id`.
fn built_in_use(product: &ValidatedProduct, id: &str) -> Option<&'static str> {
    product.board_id_use(id).map(|used| match used {
        BoardIdUse::Id => "a built-in board already uses this id",
        BoardIdUse::FormerId => {
            "a built-in board used this id before and its saved settings are read as that board's"
        }
    })
}

/// Agent/MCP routing uses static IDs. Retain each ID once per process, never
/// titles, descriptions or a fresh identity allocation on each reload.
fn intern_id(value: &str) -> &'static str {
    static IDS: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let mut ids = IDS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(id) = ids.get(value) {
        return id;
    }
    let id = Box::leak(value.to_owned().into_boxed_str());
    ids.insert(id);
    id
}
