//! The Subspace App's product descriptor: its Spaces, themes, bundle id and
//! release stream. Future products are not found by scanning `~/.config`.

use subspace_framework::product::{
    BoardDescriptor, Channel, DEFAULT_THEME, DEFAULT_THEME_DOCUMENT, ProductDescriptor,
    UpdateSource, asset_stem, is_integration_version,
};

pub const SUBSPACE_PRODUCT_ID: &str = "subspace";
pub const SUBSPACE_BINARY_NAME: &str = "subspace";
/// The app's version, which decides whether this build updates itself.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The theme a new install opens in: the first desktop start of an install
/// with no config or state file records it as the saved theme. Twilight Two
/// has only a dark variant, so a new install opens dark whatever the system
/// appearance.
pub const NEW_INSTALL_THEME: &str = "Twilight Two";
/// The bundled theme document that holds [`NEW_INSTALL_THEME`].
const NEW_INSTALL_THEME_DOCUMENT: &str = "twilight-two";

const SUBSPACE_THEMES: &[&str] = &[
    "one-dark",
    DEFAULT_THEME_DOCUMENT,
    "subspace-channel",
    "macos-classic-dark-remix",
    NEW_INSTALL_THEME_DOCUMENT,
    "adventure",
    "alduin",
    "asciinema",
    "aurora",
    "ayu",
    "catppuccin",
    "everforest",
    "fahrenheit",
    "flexoki",
    "gruvbox",
    "harper",
    "hybrid",
    "jellybeans",
    "kibble",
    "macos-classic",
    "mellifluous",
    "molokai",
    "solarized",
    "spaceduck",
    "tokyonight",
    "twilight",
];

/// Full Subspace for this build's channel.
pub fn subspace_descriptor() -> ProductDescriptor {
    subspace_descriptor_for(Channel::current())
}

/// Subspace: every board this build includes, with board commands under
/// their ids. The display name, bundle id and release stream follow
/// `channel`. Space Lab and the Component gallery are listed only with the
/// `space-lab` and `component-gallery` features, which the app turns on with
/// its own, and start off in every build profile.
pub fn subspace_descriptor_for(channel: Channel) -> ProductDescriptor {
    descriptor_for_version(
        channel,
        VERSION,
        cfg!(all(
            feature = "agentsview",
            any(target_os = "macos", windows)
        )),
    )
}

/// Metadata for a release built with the app's default features. The metadata
/// helper runs on the packaging host, even when the app is cross-compiled.
pub fn subspace_descriptor_for_release_target(target: &str) -> ProductDescriptor {
    descriptor_for_version(Channel::current(), VERSION, release_has_agentsview(target))
}

fn release_has_agentsview(target: &str) -> bool {
    target.ends_with("-apple-darwin") || target.contains("-windows-")
}

fn descriptor_for_version(channel: Channel, version: &str, agentsview: bool) -> ProductDescriptor {
    let mut boards = vec![
        board("irc", true),
        board("issues", true),
        board("github", true),
        #[cfg(feature = "copilot")]
        board("github-copilot-agents", false),
        board("projects", true),
        board("decisions", true),
        #[cfg(all(feature = "space-lab", target_os = "macos", target_arch = "aarch64"))]
        board("space-lab", false),
    ];
    if agentsview {
        boards.push(board("agentsview", true));
    }
    boards.push(board("actions", true));
    #[cfg(feature = "component-gallery")]
    boards.push(board("component-gallery", false));

    ProductDescriptor {
        id: SUBSPACE_PRODUCT_ID.into(),
        binary_name: SUBSPACE_BINARY_NAME.into(),
        display_name: channel.display_name("Subspace"),
        bundle_id: channel.bundle_id("com.aiconsultancy.subspace"),
        release_repository: RELEASE_REPOSITORY.into(),
        icon_paths: icon_paths(channel, "subspace", "subspace-app"),
        credits: Vec::new(),
        themes: SUBSPACE_THEMES.iter().map(|id| (*id).to_owned()).collect(),
        // What an install with no saved theme shows. It stays the Framework
        // default so that installs from before Twilight Two keep their look;
        // new installs get NEW_INSTALL_THEME recorded instead.
        default_theme: Some(DEFAULT_THEME.into()),
        command_mount: Default::default(),
        boards,
        update: (!is_integration_version(version)).then(|| {
            release_source(
                channel,
                "subspace-desktop-v",
                SUBSPACE_BINARY_NAME,
                "Subspace",
                "SUBSPACE_GITHUB_TOKEN",
            )
        }),
    }
}

/// Subspace's repository for releases, updates, issue reports and web packages.
/// Forks change this one value; bundle IDs must remain unchanged.
pub const RELEASE_REPOSITORY: &str = "aiconsultancy/subspace";

fn icon_paths(channel: Channel, product: &str, stable_svg: &str) -> Vec<String> {
    let native = if channel == Channel::Nightly {
        format!("{product}-nightly")
    } else {
        product.to_owned()
    };
    let svg = if channel == Channel::Nightly {
        native.as_str()
    } else {
        stable_svg
    };
    vec![
        format!("assets/{native}.icns"),
        format!("assets/{native}.ico"),
        format!("assets/{svg}.svg"),
    ]
}

fn release_source(
    channel: Channel,
    tag_prefix: &str,
    archive_prefix: &str,
    app_bundle: &str,
    token_variable: &str,
) -> UpdateSource {
    UpdateSource {
        repository: RELEASE_REPOSITORY.into(),
        tag_prefix: tag_prefix.into(),
        archive_prefix: archive_prefix.into(),
        app_bundle: asset_stem(&channel.display_name(app_bundle)),
        token_variable: token_variable.into(),
        channel,
        exclusive: false,
    }
}

/// Declared known-product order.
pub fn known_product_descriptors() -> [ProductDescriptor; 1] {
    [subspace_descriptor()]
}

fn board(id: &str, default_enabled: bool) -> BoardDescriptor {
    BoardDescriptor {
        id: id.into(),
        aliases: Vec::new(),
        former_ids: Vec::new(),
        default_enabled,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn agentsview_is_listed_only_in_supported_feature_builds() {
        let descriptor = super::subspace_descriptor();
        let agentsview = descriptor
            .boards
            .iter()
            .find(|board| board.id == "agentsview");
        assert_eq!(
            agentsview.map(|board| board.default_enabled),
            cfg!(all(
                feature = "agentsview",
                any(target_os = "macos", windows)
            ))
            .then_some(true)
        );
    }

    #[test]
    fn packaged_agentsview_follows_the_release_target() {
        for (target, enabled) in [
            ("aarch64-apple-darwin", true),
            ("x86_64-pc-windows-msvc", true),
            ("x86_64-unknown-linux-gnu", false),
            ("aarch64-unknown-linux-gnu", false),
        ] {
            let descriptor = super::subspace_descriptor_for_release_target(target);
            let view = descriptor
                .boards
                .iter()
                .find(|board| board.id == "agentsview");
            assert_eq!(
                view.map(|board| board.default_enabled),
                enabled.then_some(true),
                "{target}"
            );
        }
    }

    #[test]
    fn space_lab_is_listed_only_with_its_feature_and_starts_off() {
        let full = super::subspace_descriptor();
        let lab = full.boards.iter().find(|board| board.id == "space-lab");
        assert_eq!(
            lab.is_some(),
            cfg!(all(
                feature = "space-lab",
                target_os = "macos",
                target_arch = "aarch64"
            ))
        );
        if let Some(lab) = lab {
            // Off in every build profile, debug included.
            assert!(!lab.default_enabled);
        }
    }

    #[test]
    fn actions_is_default_on_and_listed_before_the_gallery() {
        let full = super::subspace_descriptor();
        let actions = full
            .boards
            .iter()
            .find(|board| board.id == "actions")
            .unwrap();
        assert!(actions.default_enabled);
        let ids: Vec<_> = full.boards.iter().map(|board| board.id.as_str()).collect();
        let tail: &[&str] = if cfg!(feature = "component-gallery") {
            &["actions", "component-gallery"]
        } else {
            &["actions"]
        };
        assert_eq!(&ids[ids.len() - tail.len()..], tail);
    }

    #[test]
    fn component_gallery_is_listed_last_only_with_its_feature_and_starts_off() {
        let full = super::subspace_descriptor();
        let gallery = full
            .boards
            .iter()
            .find(|board| board.id == "component-gallery");
        assert_eq!(gallery.is_some(), cfg!(feature = "component-gallery"));
        if let Some(gallery) = gallery {
            // Off in every build profile, debug included.
            assert!(!gallery.default_enabled);
            assert_eq!(full.boards.last().unwrap().id, "component-gallery");
        }
    }
    use super::{
        NEW_INSTALL_THEME, SUBSPACE_BINARY_NAME, SUBSPACE_PRODUCT_ID, known_product_descriptors,
        subspace_descriptor, subspace_descriptor_for,
    };
    use subspace_framework::product::{
        Channel, DEFAULT_THEME, DEFAULT_THEME_DOCUMENT, ProductError, ValidatedProduct,
        product_metadata, validate_known_products, validate_product,
    };

    fn known_products() -> Result<Vec<ValidatedProduct>, ProductError> {
        validate_known_products(&known_product_descriptors())
    }

    #[test]
    fn integration_builds_keep_stable_identity_without_an_update_source() {
        let stable = subspace_descriptor_for(Channel::Stable);
        let integration = super::descriptor_for_version(
            Channel::Stable,
            "0.3.0-feat-subspace-desktop.202611151345.gabc1234",
            false,
        );
        assert!(integration.update.is_none());
        assert_eq!(integration.release_repository, super::RELEASE_REPOSITORY);
        assert_eq!(integration.display_name, stable.display_name);
        assert_eq!(integration.bundle_id, stable.bundle_id);
        assert_eq!(integration.icon_paths, stable.icon_paths);
        assert!(validate_product(&integration).is_ok());
    }

    #[test]
    fn only_nightly_selects_nightly_os_icons_and_stable_native_names_remain_compatible() {
        for channel in [Channel::Stable, Channel::Alpha, Channel::Beta] {
            assert_eq!(
                subspace_descriptor_for(channel).icon_paths,
                [
                    "assets/subspace.icns",
                    "assets/subspace.ico",
                    "assets/subspace-app.svg",
                ]
            );
        }
        assert_eq!(
            subspace_descriptor_for(Channel::Nightly).icon_paths,
            [
                "assets/subspace-nightly.icns",
                "assets/subspace-nightly.ico",
                "assets/subspace-nightly.svg",
            ]
        );
    }

    #[test]
    fn the_app_updates_from_its_release_stream() {
        let products = known_products().unwrap();
        let subspace = products[0].update.as_ref().unwrap();
        assert_eq!(subspace.repository, super::RELEASE_REPOSITORY);
        assert_eq!(subspace.tag_prefix, "subspace-desktop-v");
        assert_eq!(subspace.archive_prefix, SUBSPACE_BINARY_NAME);
        assert_eq!(subspace.app_bundle, "Subspace");
        assert_eq!(subspace.token_variable, "SUBSPACE_GITHUB_TOKEN");
    }

    #[test]
    fn each_channel_gives_the_app_its_own_identity_and_release_stream() {
        let expected = [
            (Channel::Stable, "Subspace", "", "Subspace"),
            (
                Channel::Nightly,
                "Subspace (Nightly)",
                ".nightly",
                "Subspace-Nightly",
            ),
            (
                Channel::Alpha,
                "Subspace (Alpha)",
                ".alpha",
                "Subspace-Alpha",
            ),
            (Channel::Beta, "Subspace (Beta)", ".beta", "Subspace-Beta"),
        ];
        for (channel, display, bundle_suffix, stem) in expected {
            let subspace = subspace_descriptor_for(channel);
            assert_eq!(subspace.display_name, display);
            assert_eq!(subspace.binary_name, SUBSPACE_BINARY_NAME);
            assert_eq!(
                subspace.bundle_id,
                format!("com.aiconsultancy.subspace{bundle_suffix}")
            );
            let source = subspace.update.as_ref().unwrap();
            assert_eq!(source.app_bundle, stem);
            assert_eq!(source.channel, channel);
            assert_eq!(source.repository, subspace.release_repository);
            assert_eq!(subspace.release_repository, super::RELEASE_REPOSITORY);
            assert_eq!(source.tag_prefix, "subspace-desktop-v");
            assert!(validate_known_products(&[subspace]).is_ok());
        }
    }

    #[test]
    fn the_default_build_uses_its_own_channel() {
        assert_eq!(
            subspace_descriptor(),
            subspace_descriptor_for(Channel::current())
        );
    }

    #[test]
    fn known_products_validate_in_declared_order() {
        let products = known_products().unwrap();
        assert_eq!(products[0].id.as_str(), SUBSPACE_PRODUCT_ID);
        assert_eq!(products[0].binary_name.as_str(), SUBSPACE_BINARY_NAME);
        assert_eq!(products[0].default_theme, DEFAULT_THEME);
        assert!(
            products[0]
                .themes
                .iter()
                .any(|theme| theme.as_str() == DEFAULT_THEME_DOCUMENT)
        );
        assert_eq!(known_product_descriptors().len(), 1);
        assert_eq!(subspace_descriptor().id, SUBSPACE_PRODUCT_ID);
    }

    #[test]
    fn subspace_boards_keep_the_declared_order() {
        let products = known_products().unwrap();
        let board_ids = products[0]
            .boards
            .iter()
            .map(|board| board.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            board_ids,
            [
                "irc",
                "issues",
                "github",
                #[cfg(feature = "copilot")]
                "github-copilot-agents",
                "projects",
                "decisions",
                #[cfg(all(feature = "space-lab", target_os = "macos", target_arch = "aarch64"))]
                "space-lab",
                #[cfg(all(feature = "agentsview", any(target_os = "macos", windows)))]
                "agentsview",
                "actions",
                #[cfg(feature = "component-gallery")]
                "component-gallery"
            ]
        );
    }

    #[test]
    fn board_defaults_follow_the_product_contract() {
        let products = known_products().unwrap();
        for (id, enabled) in [
            ("irc", true),
            ("issues", true),
            ("projects", true),
            ("decisions", true),
            ("github", true),
            #[cfg(feature = "copilot")]
            ("github-copilot-agents", false),
            ("actions", true),
        ] {
            let board = products[0]
                .boards
                .iter()
                .find(|board| board.id.as_str() == id)
                .unwrap();
            assert_eq!(board.default_enabled, enabled, "{id}");
        }
        // The unreleased Spaces, listed only with their features, start off.
        for id in ["space-lab", "component-gallery"] {
            if let Some(board) = products[0]
                .boards
                .iter()
                .find(|board| board.id.as_str() == id)
            {
                assert!(!board.default_enabled, "{id}");
            }
        }
    }

    /// New installs open in Twilight Two; an install with no saved theme
    /// keeps Cupertino Dark, the Framework default, as before.
    #[test]
    fn new_installs_open_in_twilight_two_and_unsaved_installs_keep_cupertino_dark() {
        assert_eq!(NEW_INSTALL_THEME, "Twilight Two");
        assert_eq!(DEFAULT_THEME, "Cupertino Dark");
        assert_eq!(DEFAULT_THEME_DOCUMENT, "cupertino");
        let products = known_products().unwrap();
        let descriptors = known_product_descriptors();
        for (product, descriptor) in products.iter().zip(descriptors.iter()) {
            assert_eq!(product.default_theme, DEFAULT_THEME);
            assert_eq!(descriptor.default_theme.as_deref(), Some(DEFAULT_THEME));
            assert!(
                !descriptor
                    .themes
                    .iter()
                    .any(|theme| theme.contains("shell"))
            );
            for id in ["one-dark", DEFAULT_THEME_DOCUMENT, "twilight-two"] {
                assert!(
                    descriptor.themes.iter().any(|theme| theme == id),
                    "{}: {id}",
                    descriptor.id
                );
            }
        }
    }

    /// Channels, Issues and Decisions start on in every build, release builds
    /// included; a saved choice in the config file still wins.
    #[test]
    fn channels_issues_and_decisions_start_on_in_every_build() {
        let defaults = super::subspace_descriptor()
            .boards
            .iter()
            .filter(|board| matches!(board.id.as_str(), "irc" | "issues" | "decisions"))
            .map(|board| board.default_enabled)
            .collect::<Vec<_>>();
        assert_eq!(defaults, vec![true; 3]);
    }

    #[test]
    fn product_manifests_match_their_validated_descriptors() {
        for descriptor in known_product_descriptors() {
            let metadata = product_metadata(&descriptor, "1.1.0").unwrap();
            assert_eq!(metadata["binary_name"], descriptor.binary_name);
            assert_eq!(metadata["display_name"], descriptor.display_name);
            assert_eq!(
                metadata["boards"].as_array().unwrap().len(),
                descriptor.boards.len()
            );
            assert_eq!(
                metadata["subspace_version"],
                subspace_framework::product::SUBSPACE_VERSION
            );
            assert_eq!(
                metadata["asset_stem"],
                descriptor.update.as_ref().unwrap().app_bundle
            );
        }
    }
}
