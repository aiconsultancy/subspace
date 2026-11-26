//! Product-supplied factories, selected in the descriptor's switcher order.

mod background;
mod configuration;
mod configured;
mod settings;
mod validation;

use gpui_kit::{App, Global, Window};
use std::borrow::Cow;
use std::sync::Arc;
use subspace_agent_tools::{BoardDoc, Catalog, ToolEnvironment, ToolProvider};
use subspace_core::config::Config;
use subspace_dashboard::settings::{BoardFields, BoardSettings};
use subspace_dashboard::{Dashboard, DashboardHandle};
use subspace_product::ValidatedProduct;

pub(crate) use background::sync_backgrounds;
pub use configuration::BoardConfiguration;
pub use configured::ConfiguredBoards;
pub use configured::reload_configured_boards;
pub(crate) use configured::retain_provider;
#[cfg(test)]
pub(crate) use configured::{apply_board_selection, forget_provider};
pub(crate) use configured::{apply_running_config, desktop_sets, enables_a_board};

/// Creates a board's view. Compiled-in boards use `DashboardHandle::new`;
/// a configured board captures its own settings.
pub type BoardFactory = Arc<dyn Fn(&mut Window, &mut App) -> DashboardHandle + Send + Sync>;
pub type BoardFieldSource = Arc<dyn Fn() -> Option<BoardFields> + Send + Sync>;

#[derive(Clone)]
#[non_exhaustive]
pub struct BoardRegistration {
    pub id: &'static str,
    pub title: Cow<'static, str>,
    pub description: Cow<'static, str>,
    /// The board's monochrome switcher SVG, also used by Settings by default.
    pub icon: Option<&'static str>,
    /// The board's compiled-in Markdown help, empty when it has none.
    pub help: &'static str,
    pub help_images: &'static [subspace_dashboard::HelpImage],
    pub searchable: bool,
    pub create: BoardFactory,
    /// The board's agent tools; callable without GPUI.
    pub tools: ToolProvider,
    /// Static settings metadata captured without constructing the board.
    pub settings: BoardSettings,
    /// Settings the board declares at run time, such as from a manifest.
    pub fields: Option<BoardFieldSource>,
    pub settings_editor: Option<subspace_dashboard::settings::SettingsEditorFactory>,
    pub start_background: fn(&mut App),
    pub stop_background: fn(&mut App),
    /// Settings hides the board's view instead of dropping it when it
    /// disables the board: see `Dashboard::RETAIN_WHEN_DISABLED`.
    pub retain_when_disabled: bool,
    pub configuration: Option<BoardConfiguration>,
    pub(crate) default_order: usize,
    pub(crate) artwork: subspace_dashboard::BoardArtwork,
    pub(crate) connection: subspace_dashboard::settings::ConnectionSettings,
    /// The board's contract declarations, read without creating the board.
    pub contracts: subspace_dashboard::contracts::BoardContracts,
}

impl BoardRegistration {
    pub fn new<D: Dashboard>() -> Self {
        Self {
            id: D::ID,
            title: D::TITLE.into(),
            description: D::DESCRIPTION.into(),
            icon: D::ICON,
            help: D::HELP_MARKDOWN,
            help_images: D::HELP_IMAGES,
            searchable: D::SEARCHABLE,
            create: Arc::new(DashboardHandle::new::<D>),
            tools: D::agent_tools,
            settings: D::settings(),
            fields: None,
            settings_editor: D::settings_editor(),
            start_background: D::start_background,
            stop_background: D::stop_background,
            retain_when_disabled: D::RETAIN_WHEN_DISABLED,
            configuration: None,
            default_order: usize::MAX,
            artwork: D::ARTWORK,
            connection: D::connection_settings(),
            contracts: D::contracts(),
        }
    }

    pub fn description(mut self, description: &'static str) -> Self {
        self.description = description.into();
        self
    }

    /// The position used when no personal Space order has been saved.
    // qual:api - Products rank their built-in Spaces, including configured ones.
    pub fn with_default_order(mut self, order: usize) -> Self {
        self.default_order = order;
        self
    }

    pub fn artwork(&self) -> subspace_dashboard::BoardArtwork {
        self.artwork
    }

    pub fn connection_settings(&self) -> subspace_dashboard::settings::ConnectionSettings {
        self.connection
    }

    pub fn doc(&self) -> Option<BoardDoc> {
        let Cow::Borrowed(title) = &self.title else {
            return None;
        };
        Some(BoardDoc {
            id: self.id,
            title,
            markdown: self.help,
            images: self.help_images,
        })
    }
}

pub(crate) struct BoardFactories(pub Vec<BoardRegistration>);

impl Global for BoardFactories {}

/// Every board included by the product, even when config disables it.
pub(crate) struct IncludedBoards(pub Vec<BoardRegistration>);

impl Global for IncludedBoards {}

pub(crate) struct RegistrationSets {
    pub included: Vec<BoardRegistration>,
    pub active: Vec<BoardRegistration>,
}

/// The product's boards that the config file leaves active, in switcher
/// order. A board the config disables is left out everywhere; a board the
/// product does not list cannot be added by the config file.
pub(crate) fn selected_registrations(
    product: &ValidatedProduct,
    available: &[BoardRegistration],
    config: &Config,
) -> Result<Vec<BoardRegistration>, String> {
    Ok(registration_sets(product, available, config)?.active)
}

pub(crate) fn registration_sets(
    product: &ValidatedProduct,
    available: &[BoardRegistration],
    config: &Config,
) -> Result<RegistrationSets, String> {
    registration_sets_with(product, available, Vec::new(), config)
}

/// As [`registration_sets`], with the app's configured boards after its
/// own. A configured board is enabled unless the config file turns it off.
pub(crate) fn registration_sets_with(
    product: &ValidatedProduct,
    available: &[BoardRegistration],
    configured: Vec<BoardRegistration>,
    config: &Config,
) -> Result<RegistrationSets, String> {
    reject_duplicate_registrations(available)?;
    let mut included = product
        .boards
        .iter()
        .map(|board| {
            available
                .iter()
                .find(|registration| registration.id == board.id.as_str())
                .cloned()
                .ok_or_else(|| format!("no GUI provider for board {}", board.id.as_str()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    included.extend(configured);
    included.sort_by_key(|board| board.default_order);
    let order = normalize_order(&included, &config.board_order);
    included.sort_by_key(|board| {
        order
            .iter()
            .position(|id| id == board.id)
            .unwrap_or(usize::MAX)
    });
    validation::validate(&included)?;
    let active = included
        .iter()
        .filter(|board| {
            let default_enabled = product
                .boards
                .iter()
                .find(|descriptor| descriptor.id.as_str() == board.id)
                .is_none_or(|descriptor| descriptor.default_enabled);
            config.board_enabled_or(board.id, default_enabled)
        })
        .cloned()
        .collect();
    Ok(RegistrationSets { included, active })
}

pub(crate) fn normalize_order(boards: &[BoardRegistration], saved: &[String]) -> Vec<String> {
    let available: Vec<_> = boards.iter().map(|board| board.id).collect();
    let saved: Vec<_> = saved.iter().map(String::as_str).collect();
    let mut result = Vec::with_capacity(available.len());
    for id in saved {
        if let Some(found) = available.iter().find(|candidate| **candidate == id)
            && !result.iter().any(|saved| saved == found)
        {
            result.push((*found).to_owned());
        }
    }
    for id in available {
        if !result.iter().any(|saved| saved == id) {
            result.push(id.to_owned());
        }
    }
    result
}

fn reject_duplicate_registrations(available: &[BoardRegistration]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for board in available {
        if !ids.insert(board.id) {
            return Err(format!("duplicate GUI provider for board {}", board.id));
        }
    }
    Ok(())
}

/// The agent tools of `boards`, in their order, with the board help tool.
pub(crate) fn tool_catalog(
    boards: &[BoardRegistration],
    environment: &ToolEnvironment,
) -> Result<Catalog, String> {
    let providers: Vec<ToolProvider> = boards.iter().map(|board| board.tools).collect();
    let docs: Vec<BoardDoc> = boards.iter().filter_map(BoardRegistration::doc).collect();
    subspace_agent_tools::catalog_with_help(&providers, docs, environment)
}

/// The boards as the MCP server sees them.
pub(crate) fn mcp_boards(boards: &[BoardRegistration]) -> Vec<subspace_mcp::Board> {
    boards
        .iter()
        .map(|board| subspace_mcp::Board {
            id: board.id,
            // MCP's static catalog label cannot borrow an editable title.
            title: match &board.title {
                Cow::Borrowed(title) => title,
                Cow::Owned(_) => board.id,
            },
            description: match &board.description {
                Cow::Borrowed(description) => description,
                Cow::Owned(_) => "",
            },
            help: board.help,
            help_images: board.help_images,
            tools: board.tools,
        })
        .collect()
}

/// Included board metadata. The fallback supports focused tests that install
/// the older globals only; production always installs `IncludedBoards`.
pub(crate) fn included_registrations(cx: &App) -> Vec<BoardRegistration> {
    cx.try_global::<IncludedBoards>()
        .map(|boards| boards.0.clone())
        .or_else(|| {
            cx.try_global::<BoardFactories>()
                .map(|boards| boards.0.clone())
        })
        .unwrap_or_default()
}

pub fn dashboards(window: &mut Window, cx: &mut App) -> Vec<DashboardHandle> {
    let factories = cx.global::<BoardFactories>().0.clone();
    factories
        .iter()
        .map(|registration| {
            let mut dashboard = (registration.create)(window, cx);
            dashboard.id = registration.id;
            dashboard.title = registration.title.to_string().into();
            dashboard.description = registration.description.to_string().into();
            dashboard
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        BoardRegistration, mcp_boards, registration_sets, selected_registrations, tool_catalog,
    };
    use gpui_kit::{
        App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, Window, div,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use subspace_agent_tools::{AgentTool, HELP_TOOL, ToolEnvironment};
    use subspace_core::config::Config;
    use subspace_core::features::{FeatureSpec, FeatureStage};
    use subspace_dashboard::settings::{BoardSettings, SettingsSection};
    use subspace_dashboard::{Dashboard, DashboardEvent, DashboardHandle};
    use subspace_lucide::IconName;
    use subspace_product::{BoardDescriptor, validate_product};

    const GRADUATED: FeatureSpec = FeatureSpec {
        id: "board.future-board.regular",
        title: "Regular feature",
        description: "A graduated fixture feature.",
        stage: FeatureStage::Stable,
        default_enabled: true,
        toggleable: true,
    };
    const REMOVED: FeatureSpec = FeatureSpec {
        id: "board.future-board.removed",
        title: "Removed feature",
        description: "A removed fixture feature.",
        stage: FeatureStage::Removed,
        default_enabled: false,
        toggleable: false,
    };
    const EXPERIMENT: FeatureSpec = FeatureSpec {
        id: "board.future-board.preview",
        title: "Preview",
        description: "A nested fixture experiment.",
        stage: FeatureStage::Experimental,
        default_enabled: false,
        toggleable: true,
    };
    const NESTED: &[SettingsSection] = &[SettingsSection {
        id: "nested",
        title: "Nested",
        icon: IconName::Settings,
        children: &[],
        features: &[EXPERIMENT],
    }];
    const SECTIONS: &[SettingsSection] = &[SettingsSection {
        id: "advanced",
        title: "Advanced",
        icon: IconName::Settings,
        children: NESTED,
        features: &[],
    }];

    struct FutureBoard {
        focus: FocusHandle,
    }

    impl EventEmitter<DashboardEvent> for FutureBoard {}

    impl Focusable for FutureBoard {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }

    impl Render for FutureBoard {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    impl Dashboard for FutureBoard {
        const ID: &'static str = "future-board";
        const ARTWORK: subspace_dashboard::BoardArtwork =
            subspace_dashboard::BoardArtwork::new(&[("fixture/board.svg", b"board artwork")]);
        const TITLE: &'static str = "Future Board";
        const ICON: Option<&'static str> = Some("future.svg");

        fn settings() -> BoardSettings {
            BoardSettings {
                icon: Some(IconName::Settings),
                description: "",
                sections: SECTIONS,
                features: &[GRADUATED, REMOVED],
                notifications: false,
                notification_defaults: Default::default(),
                background_updates: false,
                store_by_default: false,
                live_features: &[],
            }
        }

        fn connection_settings() -> subspace_dashboard::settings::ConnectionSettings {
            subspace_dashboard::settings::ConnectionSettings::new(&[
                subspace_dashboard::settings::ConnectionField::GitHubCredentials,
            ])
        }

        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            panic!(
                "registration must not construct FutureBoard: {:?}",
                cx.entity_id()
            )
        }

        fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    }

    fn unused_factory(_: &mut Window, _: &mut App) -> DashboardHandle {
        panic!("selection tests must not initialize a dashboard")
    }

    fn no_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        Vec::new()
    }

    #[test]
    fn mcp_catalog_keeps_static_titles_and_uses_stable_ids_for_editable_titles() {
        const IMAGES: &[subspace_dashboard::HelpImage] = &[subspace_dashboard::HelpImage::new(
            "guide",
            "Guide",
            "image/png",
            b"test image",
        )];
        let mut native = BoardRegistration::new::<FutureBoard>();
        native.help_images = IMAGES;
        let mut configured = board("configured-fixture");
        configured.title = String::from("Editable title").into();
        configured.description = String::from("Editable description").into();
        assert_eq!(native.doc().unwrap().images, IMAGES);
        assert!(configured.doc().is_none());
        let catalog = mcp_boards(&[native.description("Future work"), configured]);
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].title, "Future Board");
        assert_eq!(catalog[0].description, "Future work");
        assert_eq!(catalog[1].description, "");
        assert_eq!(catalog[0].help_images, IMAGES);
        assert_eq!(catalog[1].id, "configured-fixture");
        assert_eq!(catalog[1].title, "configured-fixture");
    }

    #[test]
    fn native_and_configured_registrations_capture_the_boards_artwork() {
        let native = BoardRegistration::new::<FutureBoard>();
        let configured = BoardRegistration::configured::<FutureBoard>(
            "configured-art",
            "Artwork",
            "Fixture",
            unused_factory,
        );
        assert_eq!(native.artwork().entries(), FutureBoard::ARTWORK.entries());
        assert_eq!(configured.artwork().entries(), native.artwork().entries());
        assert_eq!(
            native.connection_settings(),
            FutureBoard::connection_settings()
        );
        assert_eq!(
            configured.connection_settings(),
            native.connection_settings()
        );
    }

    #[test]
    fn artwork_collisions_keep_product_first_then_declaration_and_registration_order() {
        use gpui_kit::AssetSource as _;
        use subspace_dashboard::BoardArtwork;
        const FIRST: &[(&str, &[u8])] = &[("fixture/collision", b"first")];
        const SECOND: &[(&str, &[u8])] = &[("fixture/collision", b"second")];
        let mut late = board("late");
        late.artwork = BoardArtwork::new(SECOND).with_order(2);
        let mut early = board("early");
        early.artwork = BoardArtwork::new(FIRST).with_order(1);
        let source = crate::assets::Assets::new(&[], &[late.clone(), early.clone()]);
        assert_eq!(
            source.load("fixture/collision").unwrap().unwrap().as_ref(),
            b"first"
        );
        assert_eq!(source.list("fixture/").unwrap().len(), 2);
        let mut tied = late;
        tied.artwork = BoardArtwork::new(SECOND).with_order(1);
        let source = crate::assets::Assets::new(&[], &[tied.clone(), early.clone()]);
        assert_eq!(
            source.load("fixture/collision").unwrap().unwrap().as_ref(),
            b"second"
        );
        let source = crate::assets::Assets::new(FIRST, &[tied, early]);
        assert_eq!(
            source.load("fixture/collision").unwrap().unwrap().as_ref(),
            b"first"
        );
    }

    fn board(id: &'static str) -> BoardRegistration {
        BoardRegistration {
            id,
            title: id.into(),
            description: id.into(),
            icon: None,
            help: "",
            help_images: &[],
            searchable: false,
            create: Arc::new(unused_factory),
            tools: no_tools,
            settings: BoardSettings::default(),
            settings_editor: None,
            fields: None,
            start_background: |_| {},
            stop_background: |_| {},
            retain_when_disabled: false,
            configuration: None,
            default_order: usize::MAX,
            artwork: subspace_dashboard::BoardArtwork::default(),
            connection: Default::default(),
            contracts: Default::default(),
        }
    }

    fn documented(id: &'static str) -> BoardRegistration {
        BoardRegistration {
            help: "# Help",
            ..board(id)
        }
    }

    fn config(boards: &[(&str, bool)]) -> Config {
        Config {
            boards: boards
                .iter()
                .map(|(id, enabled)| ((*id).to_owned(), *enabled))
                .collect::<BTreeMap<String, bool>>(),
            ..Default::default()
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn space_lab_defaults_off_in_release_builds_and_keeps_a_saved_choice() {
        let mut descriptor = subspace_product::fixture::descriptor();
        descriptor.boards.retain(|board| board.id == "space-lab");
        assert_eq!(descriptor.boards.len(), 1);
        let product = validate_product(&descriptor).unwrap();
        let available = [board("space-lab")];
        let initial = registration_sets(&product, &available, &Config::default()).unwrap();
        assert_eq!(ids(&initial.included), ["space-lab"]);
        assert_eq!(initial.active.is_empty(), !cfg!(debug_assertions));
        let enabled =
            selected_registrations(&product, &available, &config(&[("space-lab", true)])).unwrap();
        assert_eq!(ids(&enabled), ["space-lab"]);
        let disabled =
            selected_registrations(&product, &available, &config(&[("space-lab", false)])).unwrap();
        assert!(disabled.is_empty());
    }

    fn ids(boards: &[BoardRegistration]) -> Vec<&'static str> {
        boards.iter().map(|board| board.id).collect()
    }

    fn product(boards: &[(&str, bool)]) -> subspace_product::ValidatedProduct {
        let mut descriptor = subspace_product::fixture::descriptor();
        descriptor.boards = boards
            .iter()
            .map(|(id, default_enabled)| BoardDescriptor {
                id: (*id).into(),
                aliases: Vec::new(),
                former_ids: Vec::new(),
                default_enabled: *default_enabled,
            })
            .collect();
        validate_product(&descriptor).unwrap()
    }

    fn future_product() -> subspace_product::ValidatedProduct {
        product(&[("future-board", true)])
    }

    #[test]
    fn factories_follow_selection_order_and_reject_missing_providers() {
        let available = [board("github"), board("irc")];
        let selected = selected_registrations(
            &product(&[("irc", true), ("github", true)]),
            &available,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(ids(&selected), ["irc", "github"]);
        let known = subspace_product::fixture::products().unwrap();
        let missing = selected_registrations(&known[0], &available, &Config::default());
        assert!(matches!(missing, Err(error) if error.contains("github-copilot-agents")));
    }

    #[test]
    fn saved_order_is_normalized_before_enablement_filtering() {
        let product = product(&[("irc", true), ("github", true)]);
        let available = [board("irc"), board("github")];
        let config = Config {
            board_order: vec!["github".into(), "github".into(), "unknown".into()],
            ..Default::default()
        };
        let sets = registration_sets(&product, &available, &config).unwrap();
        assert_eq!(ids(&sets.included), ["github", "irc"]);
        assert_eq!(ids(&sets.active), ["github", "irc"]);
    }

    #[test]
    fn the_config_file_leaves_out_disabled_boards_and_cannot_add_others() {
        let product = product(&[("irc", true)]);
        let available = [board("github"), board("irc")];
        let off = config(&[("irc", false), ("github", true), ("unknown", true)]);
        let selected = selected_registrations(&product, &available, &off).unwrap();
        assert!(selected.is_empty(), "{:?}", ids(&selected));
        let on = config(&[("irc", true)]);
        assert_eq!(
            ids(&selected_registrations(&product, &available, &on).unwrap()),
            ["irc"]
        );
    }

    #[test]
    fn a_disabled_board_still_needs_settings_metadata() {
        let off = config(&[("irc", false)]);
        let result = selected_registrations(&product(&[("irc", true)]), &[board("github")], &off);
        assert!(matches!(result, Err(error) if error.contains("irc")));
    }

    #[test]
    fn future_board_registration_keeps_disabled_metadata_and_nested_sections() {
        let registration = BoardRegistration::new::<FutureBoard>();
        let sets = registration_sets(
            &future_product(),
            &[registration, board("excluded")],
            &config(&[("future-board", false)]),
        )
        .unwrap();
        assert!(sets.active.is_empty());
        assert_eq!(ids(&sets.included), ["future-board"]);
        let settings = sets.included[0].settings;
        assert_eq!(sets.included[0].icon, Some("future.svg"));
        assert_eq!(settings.features, &[GRADUATED, REMOVED]);
        assert_eq!(settings.sections[0].children[0].features, &[EXPERIMENT]);
    }

    #[test]
    fn board_defaults_allow_missing_and_explicit_config_choices() {
        let product = product(&[("irc", false)]);
        let available = [board("irc")];

        assert!(
            registration_sets(&product, &available, &Config::default())
                .unwrap()
                .active
                .is_empty()
        );
        assert_eq!(
            ids(
                &registration_sets(&product, &available, &config(&[("irc", true)]))
                    .unwrap()
                    .active
            ),
            ["irc"]
        );
        assert!(
            registration_sets(&product, &available, &config(&[("irc", false)]))
                .unwrap()
                .active
                .is_empty()
        );
    }

    #[test]
    fn invalid_owner_duplicate_keys_and_duplicate_registrations_are_rejected() {
        let product = future_product();
        let mut wrong_owner = BoardRegistration::new::<FutureBoard>();
        wrong_owner.settings.features = &[FeatureSpec {
            id: "board.other.preview",
            ..EXPERIMENT
        }];
        assert!(registration_sets(&product, &[wrong_owner], &Config::default()).is_err());

        let mut duplicate = BoardRegistration::new::<FutureBoard>();
        duplicate.settings.features = &[EXPERIMENT];
        assert!(registration_sets(&product, &[duplicate], &Config::default()).is_err());

        let registration = BoardRegistration::new::<FutureBoard>();
        assert!(
            registration_sets(
                &product,
                &[registration.clone(), registration],
                &Config::default()
            )
            .is_err()
        );
    }

    fn configured(id: &str) -> BoardRegistration {
        BoardRegistration::configured::<FutureBoard>(id, "Configured", "A page", unused_factory)
    }

    #[test]
    fn configured_boards_follow_saved_order_and_are_enabled_by_default() {
        let product = future_product();
        let available = [BoardRegistration::new::<FutureBoard>()];
        let config = Config {
            board_order: vec!["wiki".into(), "future-board".into()],
            boards: BTreeMap::from([("app".to_owned(), false)]),
            ..Default::default()
        };
        let sets = super::registration_sets_with(
            &product,
            &available,
            vec![configured("app"), configured("wiki")],
            &config,
        )
        .unwrap();
        assert_eq!(ids(&sets.included), ["wiki", "future-board", "app"]);
        assert_eq!(ids(&sets.active), ["wiki", "future-board"]);
        let wiki = &sets.included[0];
        assert_eq!(
            (wiki.title.as_ref(), wiki.description.as_ref()),
            ("Configured", "A page")
        );
        assert_eq!(wiki.icon, Some("future.svg"));
        assert!(!wiki.searchable);
    }

    #[test]
    fn defaults_order_native_and_configured_boards_together() {
        let product = product(&[("irc", true), ("github", true)]);
        let available = [
            board("irc").with_default_order(0),
            board("github").with_default_order(2),
        ];
        let configured = vec![configured("console").with_default_order(1)];
        let sets =
            super::registration_sets_with(&product, &available, configured, &Config::default())
                .unwrap();
        assert_eq!(ids(&sets.active), ["irc", "console", "github"]);
    }

    #[test]
    fn a_configured_board_cannot_take_a_built_in_id() {
        let (accepted, notices) = super::configured::accept_configured(
            &future_product(),
            vec![configured("future-board"), configured("wiki")],
        );
        assert_eq!(ids(&accepted), ["wiki"]);
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("future-board"));
    }

    #[test]
    fn a_configured_board_cannot_take_a_built_in_former_id() {
        let mut descriptor = subspace_product::fixture::descriptor();
        descriptor.boards[0].former_ids = vec!["example-old".into()];
        let product = validate_product(&descriptor).unwrap();
        let (accepted, notices) = super::configured::accept_configured(
            &product,
            vec![configured("example-old"), configured("wiki")],
        );
        assert_eq!(ids(&accepted), ["wiki"]);
        assert_eq!(notices.len(), 1);
        assert!(
            notices[0].contains("example-old") && notices[0].contains("used this id before"),
            "{notices:?}"
        );
    }

    #[test]
    fn configured_boards_add_no_tools_of_their_own() {
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let catalog = tool_catalog(&[configured("a"), configured("b")], &environment).unwrap();
        assert!(catalog.is_empty());
    }

    #[test]
    fn boards_without_tools_or_help_give_an_empty_catalog() {
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let catalog = tool_catalog(&[board("github"), board("irc")], &environment).unwrap();
        assert!(catalog.is_empty());
        assert!(tool_catalog(&[], &environment).unwrap().is_empty());
    }

    #[test]
    fn a_documented_board_adds_the_shared_help_tool() {
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let catalog = tool_catalog(&[board("github"), documented("irc")], &environment).unwrap();
        assert!(catalog.get(HELP_TOOL).is_some());
        assert_eq!(catalog.tools().len(), 1);
    }

    #[gpui_kit::test]
    fn background_follows_the_active_set_and_headless_catalog_never_starts_it(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        /// Starts and stops, as counts.
        struct Calls(usize, usize);
        impl gpui_kit::Global for Calls {}
        let mut registration = board("fixture");
        registration.start_background = |cx| cx.global_mut::<Calls>().0 += 1;
        registration.stop_background = |cx| cx.global_mut::<Calls>().1 += 1;
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        assert!(
            tool_catalog(&[registration.clone()], &environment)
                .unwrap()
                .is_empty()
        );
        cx.update(|cx| {
            cx.set_global(Calls(0, 0));
            cx.set_global(super::BoardFactories(vec![registration.clone()]));
            super::sync_backgrounds(cx);
            super::sync_backgrounds(cx);
            assert_eq!((cx.global::<Calls>().0, cx.global::<Calls>().1), (1, 0));
            cx.set_global(super::BoardFactories(vec![board("other")]));
            super::sync_backgrounds(cx);
            assert_eq!((cx.global::<Calls>().0, cx.global::<Calls>().1), (1, 1));
            cx.set_global(super::BoardFactories(vec![registration]));
            super::sync_backgrounds(cx);
            assert_eq!((cx.global::<Calls>().0, cx.global::<Calls>().1), (2, 1));
        });
    }

    #[test]
    fn background_change_starts_new_boards_and_stops_removed_ones() {
        fn stopped(_: &mut App) {}
        let running = [("kept", stopped as fn(&mut App)), ("removed", stopped)];
        let change = super::background::change(&running, &[board("kept"), board("added")]);
        assert_eq!(change.start.len(), 1);
        assert_eq!(change.stop.len(), 1);
        let ids: Vec<_> = change.running.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, ["kept", "added"]);
    }
}
