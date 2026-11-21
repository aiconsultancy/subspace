//! A fixture product for tests: the Subspace App's identity, themes and
//! board list as plain data.

use crate::channel::{Channel, asset_stem};
use crate::descriptor::{
    BoardDescriptor, ProductDescriptor, ValidatedProduct, validate_known_products,
};
use crate::error::ProductError;
use crate::update::UpdateSource;
use crate::{DEFAULT_THEME, DEFAULT_THEME_DOCUMENT};

/// The fixture's product id and binary name.
pub const ID: &str = "subspace";
/// The repository the fixture's updates come from.
pub const RELEASE_REPOSITORY: &str = "example-org/example-app";

const THEMES: &[&str] = &[
    "one-dark",
    DEFAULT_THEME_DOCUMENT,
    "subspace-channel",
    "macos-classic-dark-remix",
    "twilight-two",
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

const BOARDS: &[(&str, bool)] = &[
    ("github", true),
    ("github-copilot-agents", false),
    ("irc", cfg!(debug_assertions)),
    ("issues", cfg!(debug_assertions)),
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    ("board-lab", cfg!(debug_assertions)),
    ("actions", false),
    ("component-gallery", cfg!(debug_assertions)),
];

/// The fixture for this build's channel.
pub fn descriptor() -> ProductDescriptor {
    descriptor_for(Channel::current())
}

/// The fixture; the display name, bundle id, icons and release stream follow
/// `channel`.
pub fn descriptor_for(channel: Channel) -> ProductDescriptor {
    ProductDescriptor {
        id: ID.into(),
        binary_name: ID.into(),
        display_name: channel.display_name("Subspace"),
        bundle_id: channel.bundle_id("com.example.subspace"),
        release_repository: RELEASE_REPOSITORY.into(),
        credits: Vec::new(),
        icon_paths: icon_paths(channel),
        themes: THEMES.iter().map(|id| (*id).to_owned()).collect(),
        default_theme: Some(DEFAULT_THEME.into()),
        boards: BOARDS
            .iter()
            .map(|(id, default_enabled)| BoardDescriptor {
                id: (*id).into(),
                aliases: Vec::new(),
                default_enabled: *default_enabled,
            })
            .collect(),
        update: Some(UpdateSource {
            repository: RELEASE_REPOSITORY.into(),
            tag_prefix: "subspace-desktop-v".into(),
            archive_prefix: ID.into(),
            app_bundle: asset_stem(&channel.display_name("Subspace")),
            token_variable: "SUBSPACE_GITHUB_TOKEN".into(),
            channel,
        }),
    }
}

/// The fixture, validated, as the only product.
pub fn products() -> Result<Vec<ValidatedProduct>, ProductError> {
    validate_known_products(&[descriptor()])
}

fn icon_paths(channel: Channel) -> Vec<String> {
    let (native, svg) = if channel == Channel::Nightly {
        ("subspace-nightly", "subspace-nightly")
    } else {
        ("subspace", "subspace-app")
    };
    vec![
        format!("assets/{native}.icns"),
        format!("assets/{native}.ico"),
        format!("assets/{svg}.svg"),
    ]
}
