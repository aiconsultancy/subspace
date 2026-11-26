//! Full Subspace composition. The shared shell owns all startup and window code.

mod connection;
mod fonts;
mod identity;
mod launch;
mod new_install;
#[path = "../build/settings.rs"]
mod settings;
mod sign_in;
mod surfaces;
mod web_spaces;

include!(concat!(env!("OUT_DIR"), "/organization_logo.rs"));

use std::process::ExitCode;
use std::sync::OnceLock;

use subspace_framework::github_auth::OAuthClient;
use subspace_framework::product::{BuildInfo, BuildKind};
use subspace_framework::shell::BoardRegistration;

const SETTINGS_EXTENSIONS: &[subspace_framework::shell::SettingsExtension] = &[
    subspace_framework::shell::SettingsExtension::new(
        "web-boards",
        "Web Spaces",
        subspace_framework::lucide::IconName::Globe,
        |window, cx| {
            subspace_web_board::manager::create(
                window,
                cx,
                subspace_framework::shell::reload_configured_boards,
            )
        },
    )
    .with_github_credentials(),
    // Each fleet's Fleet API token, shared by every Space that shows fleets.
    subspace_framework::shell::SettingsExtension::new(
        "fleets",
        "Fleets",
        subspace_framework::lucide::IconName::Ship,
        subspace_client_fleet::settings::create,
    ),
];

fn main() -> ExitCode {
    let mut descriptor = subspace_descriptor::subspace_descriptor();
    identity::adjust_descriptor(&mut descriptor);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let desktop = args.is_empty();
    // About, --version and upgrade read no settings, so a broken connection
    // file or variable never stands between someone and an upgrade.
    if subspace_framework::shell::reads_configuration(&descriptor, &args)
        && let Err(failure) = configure(&descriptor)
    {
        let window = desktop && !capturing();
        return connection::report(&descriptor.display_name, &failure, window);
    }
    // Only the desktop shows Spaces and themes; commands leave the config
    // and state files alone. A new install is told by having neither file,
    // so its theme is recorded before the built-in web Spaces write the
    // config file.
    if desktop {
        new_install::record_theme(&descriptor, env!("CARGO_PKG_VERSION"));
        web_spaces::install(&descriptor, env!("CARGO_PKG_VERSION"));
    }
    subspace_framework::shell::launch(
        subspace_framework::shell::ProductBuild {
            descriptor,
            display_name_prefix: Some(env!("SUBSPACE_BUILD_TITLE_PREFIX")),
            default_theme_override: option_env!("SUBSPACE_DEFAULT_THEME"),
            version: env!("CARGO_PKG_VERSION"),
            build: build_info(),
            #[cfg(not(windows))]
            search_reranker: Some(|root| {
                std::sync::Arc::new(subspace_framework::search_onnx::LocalReranker::new(root))
            }),
            #[cfg(windows)]
            search_reranker: None,
            account: identity::registration(),
            configured_boards: Some(web_boards),
            settings_extensions: settings_extensions(),
            #[cfg(feature = "terminal")]
            dock_panel: Some(subspace_terminal_panel::registration()),
            #[cfg(not(feature = "terminal"))]
            dock_panel: None,
            github_oauth: github_oauth(),
            fonts: fonts::bundled(),
            product_themes: &[],
            #[cfg(feature = "report-issue")]
            report_issue_repository: Some(env!("SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY")),
        },
        board_registrations(),
        assets(),
        subspace_framework::board_command::run,
    )
}

/// Reads the connection file, when the app's config folder has one, and
/// installs the backend the build, the file and the run's variables select.
fn configure(
    descriptor: &subspace_framework::product::ProductDescriptor,
) -> Result<(), launch::Failure> {
    let located = connection::locate(descriptor, env!("CARGO_PKG_VERSION"));
    let file = located.map_or(Ok(None), |path| {
        connection::read(&path).map_err(|message| launch::Failure::File(path, message))
    })?;
    identity::configure_backend(file.as_ref(), identity::variables())
}

/// Whether this is a capture run, which must fail on standard error with a
/// non-zero exit rather than wait in a window no one will close.
fn capturing() -> bool {
    cfg!(debug_assertions) && std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some()
}

/// The Settings extensions this build's backend offers.
fn settings_extensions() -> &'static [subspace_framework::shell::SettingsExtension] {
    static OFFERED: OnceLock<Vec<subspace_framework::shell::SettingsExtension>> = OnceLock::new();
    OFFERED.get_or_init(|| surfaces::settings_extensions(identity::backend(), SETTINGS_EXTENSIONS))
}

/// The OAuth App `build/config.rs` set, with the client secret from the
/// build environment. An empty client id builds without Sign in with GitHub.
fn github_oauth() -> Option<OAuthClient> {
    OAuthClient::new(
        env!("SUBSPACE_BUILD_GITHUB_OAUTH_CLIENT_ID"),
        option_env!("SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET"),
    )
}

/// The source repository About names.
const REPOSITORY: &str = "https://github.com/aiconsultancy/subspace";

/// The build `products/build.rs` stamped.
fn build_info() -> BuildInfo {
    BuildInfo {
        repository: Some(REPOSITORY),
        commit: env!("SUBSPACE_BUILD_COMMIT"),
        dirty: matches!(env!("SUBSPACE_BUILD_DIRTY").as_bytes(), b"true"),
        branch: env!("SUBSPACE_BUILD_BRANCH"),
        timestamp: env!("SUBSPACE_BUILD_TIMESTAMP"),
        unix_time: unix_time(env!("SUBSPACE_BUILD_UNIX")),
        target: env!("SUBSPACE_BUILD_TARGET"),
        profile: env!("SUBSPACE_BUILD_PROFILE"),
        kind: BuildKind::parse(env!("SUBSPACE_BUILD_KIND")),
        host: env!("SUBSPACE_BUILD_HOST"),
    }
}

fn unix_time(seconds: &str) -> u64 {
    seconds.parse().unwrap_or_default()
}

fn assets() -> &'static [(&'static str, &'static [u8])] {
    const ARTWORK: &[(&str, &[u8])] = &[
        ("product.svg", include_bytes!("../assets/subspace.svg")),
        (
            "product-light.svg",
            include_bytes!("../assets/product-light.svg"),
        ),
        (
            "product-dark.svg",
            include_bytes!("../assets/product-dark.svg"),
        ),
        ("organization.svg", ORGANIZATION_LOGO),
    ];
    if ORGANIZATION_LOGO.is_empty() {
        &ARTWORK[..ARTWORK.len() - 1]
    } else {
        ARTWORK
    }
}

/// The Spaces this build's backend serves, in product order.
fn board_registrations() -> Vec<BoardRegistration> {
    surfaces::spaces(identity::backend(), all_board_registrations())
}

/// Every Space this binary includes, in product order. Space Lab and the
/// Component gallery are not ready to ship: each is compiled in only with its
/// Cargo feature, which also lists it in the descriptor.
fn all_board_registrations() -> Vec<BoardRegistration> {
    vec![
        BoardRegistration::new::<subspace_github_board::Github>()
            .description("GitHub pull requests"),
        BoardRegistration::new::<subspace_github_copilot_agents_board::CopilotAgents>()
            .description("GitHub Copilot agent tasks"),
        BoardRegistration::new::<subspace_irc_board::Irc>()
            .description("IRC networks, channels and private messages"),
        BoardRegistration::new::<subspace_issues_board::Issues>()
            .description("Subspace API issues and comments"),
        BoardRegistration::new::<subspace_projects_board::Projects>(),
        BoardRegistration::new::<subspace_decisions_board::Decisions>()
            .description("Search, propose and review Subspace API decisions"),
        #[cfg(all(feature = "space-lab", target_os = "macos", target_arch = "aarch64"))]
        BoardRegistration::new::<subspace_space_lab_board::SpaceLab>()
            .description("Native dashboards and managed runtimes (alpha)"),
        BoardRegistration::new::<subspace_actions_board::Actions>()
            .description("GitHub Actions runs, deployments and releases (experimental)"),
        #[cfg(feature = "component-gallery")]
        BoardRegistration::new::<subspace_framework::component_gallery::ComponentGallery>()
            .description("Shared UI components in their states (developer)"),
    ]
}

/// One board per `webBoards` entry in the config file: a web page or a
/// local web app.
fn web_boards(config: &subspace_framework::core::config::Config) -> Vec<BoardRegistration> {
    config
        .web_boards
        .iter()
        .map(|board| {
            let page = board.clone();
            let registration = BoardRegistration::configured::<subspace_web_board::WebBoard>(
                &board.id,
                &board.title,
                &subspace_web_board::description(board),
                move |window, cx| subspace_web_board::create(&page, window, cx),
            );
            let settings = board.clone();
            registration
                .with_configuration(subspace_web_board::configuration(board))
                .with_field_source(move || subspace_web_board::settings_fields(&settings))
        })
        .collect()
}

#[cfg(test)]
mod settings_tests;

#[cfg(test)]
#[path = "../build/branding.rs"]
mod branding;
#[cfg(test)]
#[path = "../build/config.rs"]
mod config;
/// The build script's stamp and configuration, tested here because Cargo
/// does not test build scripts.
#[cfg(test)]
#[path = "../build/stamp.rs"]
mod stamp;
// The tests use the package rules; the rest is the build script's.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../build/web_spaces.rs"]
mod web_spaces_build;

#[cfg(test)]
mod tests {
    use super::{
        REPOSITORY, SETTINGS_EXTENSIONS, all_board_registrations, assets, board_registrations,
        build_info, github_oauth, unix_time, web_boards,
    };
    use crate::settings::ApiBackend;
    use std::collections::HashSet;
    use subspace_framework::product::BuildKind;

    /// Fleet Console shows in every build, before any fleet is set, so a
    /// build with no API backend can set its first fleet in Fleet Console's
    /// own settings or add one inside it.
    #[test]
    fn fleet_console_shows_before_any_fleet_is_set() {
        use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};
        let mut config = subspace_framework::core::config::Config::default();
        config.web_boards.push(WebBoard {
            id: subspace_web_board::builtin::FLEET_CONSOLE_ID.to_owned(),
            title: "Fleet Console".to_owned(),
            source: WebBoardSource::Folder(std::path::PathBuf::from("/apps/fleet-console")),
            settings: Default::default(),
            secrets: Default::default(),
            permissions: Default::default(),
        });
        let ids: Vec<_> = web_boards(&config).iter().map(|board| board.id).collect();
        assert_eq!(ids, [subspace_web_board::builtin::FLEET_CONSOLE_ID]);
    }

    #[test]
    fn a_config_without_web_boards_adds_no_boards() {
        let config = subspace_framework::core::config::Config::default();
        assert!(web_boards(&config).is_empty());
    }

    #[test]
    fn the_build_script_stamps_this_build() {
        let build = build_info();
        assert_eq!(build.repository, Some(REPOSITORY));
        assert!(!build.commit.is_empty() && !build.branch.is_empty());
        assert!(build.unix_time > 0);
        assert!(
            !build.host.is_empty()
                && build
                    .host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        );
        // Tests build the debug profile, which is a local build.
        assert_eq!(build.kind, BuildKind::Dev);
        assert_eq!(build.profile, "debug");
        assert_eq!(unix_time("not a number"), 0);
    }

    /// The build environment of the tests sets no overrides, so this is
    /// Subspace's own configuration.
    #[test]
    fn the_build_names_subspaces_oauth_app_and_issue_repository() {
        if option_env!("SUBSPACE_GITHUB_OAUTH_CLIENT_ID").is_none() {
            let client = github_oauth().unwrap();
            assert_eq!(client.id, super::config::GITHUB_OAUTH_CLIENT_ID);
        }
        if option_env!("SUBSPACE_REPORT_ISSUE_REPOSITORY").is_none() {
            assert_eq!(
                env!("SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY"),
                super::config::REPORT_ISSUE_REPOSITORY
            );
        }
    }

    #[test]
    fn composed_artwork_keeps_keys_bytes_and_legacy_listing_order() {
        use subspace_framework::shell::test_support::{AssetSource as _, Assets};
        let boards = all_board_registrations();
        let source = Assets::new(assets(), &boards);
        let expected = [
            ("product.svg", 0x779de1f19cfb06a0),
            ("product-light.svg", 0x779de1f19cfb06a0),
            ("product-dark.svg", 0x8ef38950ba8ecba6),
            ("logos/copilot-agents.svg", 0xdfe0f3122ef1418e),
            ("logos/irc.svg", 0xfeb1f97f6bd091e8),
            ("icons/issues.svg", 0x31ddc61a9106ee65),
            ("icons/workflow-16.svg", 0x1d2b94c6f8ab06c8),
            #[cfg(all(feature = "space-lab", target_os = "macos", target_arch = "aarch64"))]
            ("icons/flask-16.svg", 0x70ddc57f19dcb72c),
            ("logos/github.svg", 0x24524f670fa8f1ff),
            #[cfg(feature = "component-gallery")]
            ("icons/component.svg", 0xbc92e6d849ce1ea8),
            ("icons/decisions.svg", 0x5942352d2116e424),
        ];
        for (path, fingerprint) in expected {
            let bytes = source.load(path).unwrap().unwrap();
            assert!(!bytes.is_empty(), "{path}");
            let actual = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
            assert_eq!(actual, fingerprint, "{path}");
        }
        let mut paths = source.list("").unwrap();
        paths.retain(|path| path.as_ref() != "organization.svg");
        let logo = super::ORGANIZATION_LOGO;
        assert_eq!(
            source.load("organization.svg").ok().flatten().as_deref(),
            (!logo.is_empty()).then_some(logo)
        );
        let custom = expected.map(|(path, _)| path);
        assert_eq!(&paths[paths.len() - custom.len()..], custom);
        for board in &boards {
            assert!(
                source.load(board.icon.unwrap()).unwrap().is_some(),
                "{}",
                board.id
            );
        }
        assert!(source.load("icons/eye.svg").unwrap().is_some());
        assert!(source.load("missing/step11.svg").is_err());
    }

    #[test]
    fn registered_multi_section_board_guides_have_useful_contents() {
        let mut boards = board_registrations();
        assert!(!boards.is_empty());
        // Web boards are configured at runtime, but share this bundled guide.
        boards.push(subspace_framework::shell::BoardRegistration::new::<
            subspace_web_board::WebBoard,
        >());
        for board in boards {
            assert!(board.doc().is_some(), "{} must publish its help", board.id);
            subspace_framework::shell::test_support::assert_help_contents(board.help);
            subspace_framework::agent_tools::help_image::validate_images(board.help_images)
                .unwrap();
        }
    }

    #[cfg(all(feature = "space-lab", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn space_labs_registered_screenshot_reaches_the_native_help_image_cache() {
        let board = board_registrations()
            .into_iter()
            .find(|board| board.id == "space-lab")
            .unwrap();
        assert_eq!(board.help_images.len(), 3);
        subspace_framework::shell::test_support::check_help_image_cache(board.doc().unwrap())
            .unwrap();
    }

    /// The shared search tools, documented but served only with a search
    /// index, which the test environment does not have.
    const SEARCH_TOOLS: [&str; 3] = ["search_content", "read_search_result", "related_items"];

    /// Space Lab's tools, which the Spaces audit documents but only a build
    /// with Space Lab compiles.
    const SPACE_LAB_TOOLS: [&str; 6] = [
        "space_lab_inspect",
        "space_lab_edit",
        "space_lab_run",
        "space_lab_environment",
        "space_lab_project_inspect",
        "space_lab_project_edit",
    ];

    /// `docs/spaces-audit.md` lists every tool and action full Subspace
    /// serves, less the absent search tools and, in a build without Space
    /// Lab, Space Lab's.
    #[test]
    fn the_documented_tool_inventory_matches_the_compiled_catalog() {
        let registrations = all_board_registrations();
        let mut absent = SEARCH_TOOLS.to_vec();
        if !cfg!(all(
            feature = "space-lab",
            target_os = "macos",
            target_arch = "aarch64"
        )) {
            absent.extend(SPACE_LAB_TOOLS);
        }
        subspace_framework::agent_tools::test_support::check_documented_catalog(
            &registrations
                .iter()
                .map(|board| board.tools)
                .collect::<Vec<_>>(),
            registrations
                .iter()
                .filter_map(|board| board.doc())
                .collect(),
            include_str!("../../../docs/spaces-audit.md"),
            "Tools",
            &absent,
        )
        .unwrap();
    }

    /// `docs/spaces-audit.md` lists every contract role each board
    /// declares, as `| `board` | role | `contract@version/message` |` rows.
    #[test]
    fn the_documented_contract_inventory_matches_the_registrations() {
        let page = include_str!("../../../docs/spaces-audit.md");
        let section = page
            .split("## Declared contracts")
            .nth(1)
            .and_then(|rest| rest.split("\n#").next())
            .unwrap();
        let documented: HashSet<String> = section
            .lines()
            .filter(|line| line.starts_with("| `"))
            .map(str::to_owned)
            .collect();
        let declared: HashSet<String> = all_board_registrations()
            .iter()
            .flat_map(|board| {
                board
                    .contracts
                    .declarations()
                    .iter()
                    .map(|declaration| {
                        format!(
                            "| `{}` | {} | `{}` |",
                            board.id,
                            declaration.role().as_str(),
                            declaration.key().label()
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(documented, declared);
    }

    /// Settings hides these boards instead of closing them, so disabling
    /// one does not end its shells or project workers.
    #[test]
    fn only_boards_that_own_processes_stay_hidden_when_disabled() {
        let retained: Vec<_> = board_registrations()
            .into_iter()
            .filter(|board| board.retain_when_disabled)
            .map(|board| board.id)
            .collect();
        let expected = cfg!(all(
            feature = "space-lab",
            target_os = "macos",
            target_arch = "aarch64"
        ))
        .then_some("space-lab");
        assert_eq!(retained, expected.into_iter().collect::<Vec<_>>());
    }

    /// Issue content stays in memory: no agent
    /// tools, search indexing, notifications or background reads.
    #[test]
    fn the_issues_board_keeps_issue_content_out_of_tools_search_and_alerts() {
        let issues = all_board_registrations()
            .into_iter()
            .find(|board| board.id == "issues")
            .unwrap();
        let environment = subspace_framework::agent_tools::test_support::app_environment(
            subspace_framework::core::config::Config::default(),
        );
        assert!((issues.tools)(&environment).is_empty());
        assert!(!issues.searchable);
        assert!(!issues.settings.notifications);
        assert!(!issues.settings.background_updates);
        assert!(issues.settings_editor.is_some());
        assert_eq!(issues.icon, Some("icons/issues.svg"));
    }

    /// The shell refuses a descriptor board without a registration, and a
    /// registration the descriptor does not list never appears, so the
    /// feature that adds a Space must add both.
    #[test]
    fn the_descriptor_lists_exactly_the_registered_boards_in_order() {
        let registered: Vec<_> = board_registrations()
            .iter()
            .map(|registration| registration.id)
            .collect();
        let mut descriptor = subspace_descriptor::subspace_descriptor();
        super::identity::adjust_descriptor(&mut descriptor);
        let described: Vec<_> = descriptor
            .boards
            .iter()
            .map(|board| board.id.as_str())
            .collect();
        assert_eq!(described, registered);
    }

    /// Subspace API and NETT builds register every Space and Settings page;
    /// a build with no API backend leaves out those that need the API, and
    /// lists exactly the rest.
    #[test]
    fn each_backend_registers_its_spaces_and_settings_pages() {
        let ids = |backend| {
            super::surfaces::spaces(backend, all_board_registrations())
                .iter()
                .map(|board| board.id)
                .collect::<Vec<_>>()
        };
        let pages = |backend| {
            super::surfaces::settings_extensions(backend, SETTINGS_EXTENSIONS)
                .iter()
                .map(|extension| extension.id)
                .collect::<Vec<_>>()
        };
        let all: Vec<_> = all_board_registrations()
            .iter()
            .map(|board| board.id)
            .collect();
        for backend in [ApiBackend::Subspace, ApiBackend::Nett] {
            assert_eq!(ids(backend), all);
            assert_eq!(pages(backend), ["web-boards", "fleets"]);
        }
        let none = ids(ApiBackend::None);
        let expected: Vec<_> = all
            .iter()
            .copied()
            .filter(|id| !["issues", "projects", "decisions"].contains(id))
            .collect();
        assert_eq!(none, expected);
        for kept in ["github", "github-copilot-agents", "irc", "actions"] {
            assert!(none.contains(&kept), "{kept}");
        }
        assert_eq!(pages(ApiBackend::None), ["web-boards"]);

        // Its descriptor lists exactly those Spaces, so Settings and Add a
        // Space offer no other.
        let mut descriptor = subspace_descriptor::subspace_descriptor();
        super::surfaces::adjust_descriptor(&mut descriptor);
        let described: Vec<_> = descriptor
            .boards
            .iter()
            .map(|board| board.id.as_str())
            .collect();
        assert_eq!(described, none);

        // The stamped build serves its own backend's set.
        let stamped: Vec<_> = board_registrations().iter().map(|board| board.id).collect();
        assert_eq!(stamped, ids(super::identity::backend()));
        let offered: Vec<_> = super::settings_extensions()
            .iter()
            .map(|extension| extension.id)
            .collect();
        assert_eq!(offered, pages(super::identity::backend()));
    }

    /// A build with no API backend updates only from its own archives, in
    /// the same releases, so the standard build never replaces it; the
    /// standard build's source is unchanged.
    #[test]
    fn a_build_without_a_backend_updates_only_from_its_own_archives() {
        let original = subspace_descriptor::subspace_descriptor();
        let mut descriptor = original.clone();
        super::surfaces::adjust_descriptor(&mut descriptor);
        let (update, standard) = (descriptor.update.unwrap(), original.update.unwrap());
        assert_eq!(update.archive_prefix, "subspace-no-api");
        assert_eq!(update.app_bundle, format!("{}-NoAPI", standard.app_bundle));
        assert!(update.exclusive && !standard.exclusive);
        assert_eq!(update.tag_prefix, standard.tag_prefix, "the same releases");
        assert_eq!(standard.archive_prefix, "subspace");
    }

    /// Space Lab and the Component gallery are in the binary only with their
    /// features, which release builds do not turn on.
    #[test]
    fn the_unreleased_spaces_follow_their_features() {
        let ids: Vec<_> = board_registrations()
            .iter()
            .map(|registration| registration.id)
            .collect();
        assert_eq!(
            ids.contains(&"space-lab"),
            cfg!(all(
                feature = "space-lab",
                target_os = "macos",
                target_arch = "aarch64"
            ))
        );
        assert_eq!(
            ids.contains(&"component-gallery"),
            cfg!(feature = "component-gallery")
        );
    }

    #[cfg(feature = "component-gallery")]
    #[test]
    fn the_component_gallery_is_registered_last() {
        let boards = board_registrations();
        let gallery = boards.last().unwrap();
        assert_eq!(gallery.id, "component-gallery");
        assert_eq!(gallery.icon, Some("icons/component.svg"));
        assert!(!gallery.retain_when_disabled);
    }

    #[test]
    fn current_boards_have_unique_registered_icons() {
        let boards = board_registrations();
        let icons = boards
            .iter()
            .map(|board| board.icon.expect(board.id))
            .collect::<Vec<_>>();
        assert_eq!(icons.len(), icons.iter().collect::<HashSet<_>>().len());
        use subspace_framework::shell::test_support::{AssetSource as _, Assets};
        let source = Assets::new(assets(), &boards);
        for icon in icons {
            let svg = source.load(icon).unwrap().unwrap();
            let mut text = std::str::from_utf8(&svg).unwrap().trim_start();
            while let Some(rest) = text.strip_prefix("<!--") {
                text = rest.split_once("-->").unwrap().1.trim_start();
            }
            assert!(text.starts_with("<svg"), "{icon}");
        }
    }
}
