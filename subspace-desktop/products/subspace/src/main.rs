//! Full Subspace composition. The shared shell owns all startup and window code.

mod identity;
mod launch;
#[path = "../build/settings.rs"]
mod settings;

include!(concat!(env!("OUT_DIR"), "/organization_logo.rs"));

use std::process::ExitCode;

use subspace_framework::github_auth::OAuthClient;
use subspace_framework::product::{BuildInfo, BuildKind};
use subspace_framework::shell::BoardRegistration;

const SETTINGS_EXTENSIONS: &[subspace_framework::shell::SettingsExtension] =
    &[subspace_framework::shell::SettingsExtension::new(
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
    .with_github_credentials()];

fn main() -> ExitCode {
    if let Err(message) = identity::configure_backend(identity::variables()) {
        eprintln!("{message}");
        return ExitCode::FAILURE;
    }
    let mut descriptor = subspace_descriptor::subspace_descriptor();
    identity::adjust_descriptor(&mut descriptor);
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
            settings_extensions: SETTINGS_EXTENSIONS,
            #[cfg(feature = "terminal")]
            dock_panel: Some(subspace_terminal_panel::registration()),
            #[cfg(not(feature = "terminal"))]
            dock_panel: None,
            github_oauth: github_oauth(),
            #[cfg(feature = "report-issue")]
            report_issue_repository: Some(env!("SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY")),
        },
        board_registrations(),
        assets(),
        subspace_framework::board_command::run,
    )
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

fn board_registrations() -> Vec<BoardRegistration> {
    vec![
        BoardRegistration::new::<subspace_github_board::Github>()
            .description("GitHub pull requests"),
        BoardRegistration::new::<subspace_github_copilot_agents_board::CopilotAgents>()
            .description("GitHub Copilot agent tasks"),
        BoardRegistration::new::<subspace_irc_board::Irc>()
            .description("IRC networks, channels and private messages"),
        BoardRegistration::new::<subspace_issues_board::Issues>()
            .description("Subspace API issues and comments"),
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        BoardRegistration::new::<subspace_board_lab_board::BoardLab>()
            .description("Native dashboards and managed runtimes (alpha)"),
        BoardRegistration::new::<subspace_actions_board::Actions>()
            .description("GitHub Actions runs, deployments and releases (experimental)"),
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

#[cfg(test)]
mod tests {
    use super::{
        REPOSITORY, assets, board_registrations, build_info, github_oauth, unix_time, web_boards,
    };
    use std::collections::HashSet;
    use subspace_framework::product::BuildKind;

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
        let boards = board_registrations();
        let source = Assets::new(assets(), &boards);
        let expected = [
            ("product.svg", 0x779de1f19cfb06a0),
            ("product-light.svg", 0x779de1f19cfb06a0),
            ("product-dark.svg", 0x8ef38950ba8ecba6),
            ("logos/copilot-agents.svg", 0xdfe0f3122ef1418e),
            ("logos/irc.svg", 0xfeb1f97f6bd091e8),
            ("icons/issues.svg", 0x31ddc61a9106ee65),
            ("icons/workflow-16.svg", 0x1d2b94c6f8ab06c8),
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            ("icons/flask-16.svg", 0x70ddc57f19dcb72c),
            ("logos/github.svg", 0x24524f670fa8f1ff),
            ("icons/component.svg", 0xbc92e6d849ce1ea8),
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

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn board_labs_registered_screenshot_reaches_the_native_help_image_cache() {
        let board = board_registrations()
            .into_iter()
            .find(|board| board.id == "board-lab")
            .unwrap();
        assert_eq!(board.help_images.len(), 3);
        subspace_framework::shell::test_support::check_help_image_cache(board.doc().unwrap())
            .unwrap();
    }

    /// `docs/mcp.md` lists every tool and action full Subspace serves. The
    /// shared search tools are documented but need a search index, which the
    /// test environment does not have.
    #[test]
    fn the_documented_tool_inventory_matches_the_compiled_catalog() {
        let registrations = board_registrations();
        let absent = [
            "search_content",
            "read_search_result",
            "related_items",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_inspect",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_edit",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_run",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_environment",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_project_inspect",
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            "board_lab_project_edit",
        ];
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
        let declared: HashSet<String> = board_registrations()
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
        let expected =
            cfg!(all(target_os = "macos", target_arch = "aarch64")).then_some("board-lab");
        assert_eq!(retained, expected.into_iter().collect::<Vec<_>>());
    }

    /// Issue content stays in memory: no agent
    /// tools, search indexing, notifications or background reads.
    #[test]
    fn the_issues_board_keeps_issue_content_out_of_tools_search_and_alerts() {
        let issues = board_registrations()
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
