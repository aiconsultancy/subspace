//! Validation errors. Messages never include credentials or file
//! contents.

/// A product, board or alias failure. The `name` field is the
/// identity that failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProductError {
    #[error("unsafe identity '{name}'")]
    UnsafeIdentity { name: String },
    #[error("unsafe bundle identity '{name}'")]
    UnsafeBundleId { name: String },
    #[error("unsafe display name '{name}'")]
    UnsafeDisplayName { name: String },
    #[error("unsafe release repository '{name}'")]
    UnsafeReleaseRepository { name: String },
    #[error("unsafe update source value '{name}'")]
    UnsafeUpdateSource { name: String },
    #[error(
        "update repository '{repository}' does not match release repository '{release_repository}'"
    )]
    UpdateRepositoryMismatch {
        repository: String,
        release_repository: String,
    },
    #[error("unsafe asset path '{name}'")]
    UnsafeAssetPath { name: String },
    #[error("duplicate identity '{name}'")]
    DuplicateIdentity { name: String },
    #[error("duplicate command name '{name}'")]
    DuplicateAlias { name: String },
    #[error("command name '{name}' is reserved for the product")]
    ReservedCommand { name: String },
    #[error("root command mount requires exactly one Space")]
    RootMountConflict,
    #[error("duplicate binary name '{name}'")]
    DuplicateBinary { name: String },
    #[error("duplicate bundle identity '{name}'")]
    DuplicateBundle { name: String },
    #[error("unknown board '{name}'")]
    UnknownBoard { name: String },
    #[error("unknown theme '{name}'")]
    UnknownTheme { name: String },
    #[error("missing {name} for the product directories")]
    MissingHome { name: String },
}
