//! GPUI-free product identity, command-alias validation and configuration
//! path contract. This crate does not store credentials or depend on GPUI.

mod build;
mod channel;
mod context;
mod descriptor;
mod error;
#[cfg(any(test, feature = "test-support"))]
pub mod fixture;
mod identity;
mod integration;
mod mcp;
mod metadata;
mod paths;
mod platform;
mod update;

pub use build::{BuildInfo, BuildKind};
pub use channel::{Channel, asset_stem};
pub use context::ProductContext;
pub use descriptor::{
    BoardDescriptor, ProductDescriptor, ValidatedBoard, ValidatedProduct, validate_known_products,
    validate_product,
};
pub use error::ProductError;
pub use identity::{
    Identity, parse_asset_path, parse_bundle_id, parse_display_name, parse_identity,
};
pub use integration::is_integration_version;
pub use mcp::{MCP_COMMAND, MCP_DEFAULT_BIND, MCP_USAGE};
pub use metadata::product_metadata;
pub use paths::{ConfigRoots, board_root, product_config_roots, product_data_name};
pub use platform::{OsKind, PlatformEnv};
pub use update::{UPGRADE_COMMAND, UPGRADE_USAGE, UpdateSource, validate_update_source};

/// The Subspace Framework's version. A product has its own package version
/// and retains this value for shared-shell attribution.
pub const SUBSPACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The framework's default theme variant, by its display name.
pub const DEFAULT_THEME: &str = "Cupertino Dark";
/// The bundled theme document that contains [`DEFAULT_THEME`].
pub const DEFAULT_THEME_DOCUMENT: &str = "cupertino";
