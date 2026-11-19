//! Product and board descriptors. Validation is pure: it does not read files.

use std::collections::BTreeMap;

use crate::error::ProductError;
use crate::identity::{
    Identity, parse_asset_path, parse_bundle_id, parse_display_name, parse_identity,
};
use crate::update::{
    UpdateSource, require_release_repository, validate_release_repository, validate_update_source,
};

/// Unvalidated board input supplied by a product package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardDescriptor {
    pub id: String,
    pub aliases: Vec<String>,
    pub default_enabled: bool,
}

/// Unvalidated product input supplied by a product package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDescriptor {
    pub id: String,
    pub binary_name: String,
    pub display_name: String,
    pub bundle_id: String,
    /// GitHub `owner/name` for releases, issue reports and bundled packages.
    pub release_repository: String,
    pub icon_paths: Vec<String>,
    /// Verbatim attribution lines; an empty list omits the About credits.
    pub credits: Vec<String>,
    /// Bundled theme document ids in the product's catalogue.
    pub themes: Vec<String>,
    /// Display name of the default theme variant, such as `Cupertino Dark`.
    /// `None` selects the framework default and adds its document.
    pub default_theme: Option<String>,
    pub boards: Vec<BoardDescriptor>,
    /// Release stream for `upgrade` and update checks. `None` disables them.
    pub update: Option<UpdateSource>,
}

impl ProductDescriptor {
    /// Compile-time default-theme override, by variant display name. The host
    /// checks that a declared theme document contains it.
    pub fn apply_default_theme_override(
        &mut self,
        override_theme: Option<&str>,
    ) -> Result<(), ProductError> {
        let Some(theme) = override_theme else {
            return Ok(());
        };
        self.default_theme = Some(parse_theme_name(theme)?);
        Ok(())
    }
}

/// A board after identity and alias validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedBoard {
    pub id: Identity,
    pub aliases: Vec<Identity>,
    pub default_enabled: bool,
}

/// A product after identity, alias, theme and board validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedProduct {
    pub id: Identity,
    pub binary_name: Identity,
    pub display_name: String,
    pub bundle_id: String,
    /// GitHub `owner/name` for releases, issue reports and bundled packages.
    pub release_repository: String,
    pub icon_paths: Vec<String>,
    /// Verbatim attribution lines; an empty list omits the About credits.
    pub credits: Vec<String>,
    pub themes: Vec<Identity>,
    pub default_theme: String,
    pub boards: Vec<ValidatedBoard>,
    pub commands: BTreeMap<String, Identity>,
    pub update: Option<UpdateSource>,
}

impl ValidatedProduct {
    /// Command name to board, including each board's default id.
    // qual:api (CLI and shell resolve aliases before routing)
    pub fn board_for_command(&self, name: &str) -> Option<&ValidatedBoard> {
        let board_id = self.commands.get(name)?;
        self.boards.iter().find(|board| board.id == *board_id)
    }
}

/// Validates one product descriptor. Duplicate and unsafe names are returned
/// as the actual conflicting value.
pub fn validate_product(descriptor: &ProductDescriptor) -> Result<ValidatedProduct, ProductError> {
    let id = parse_identity(&descriptor.id)?;
    let binary_name = parse_identity(&descriptor.binary_name)?;
    let display_name = parse_display_name(&descriptor.display_name)?;
    let bundle_id = parse_bundle_id(&descriptor.bundle_id)?;
    let icon_paths = parse_asset_paths(&descriptor.icon_paths)?;
    let mut themes = parse_identities(&descriptor.themes)?;
    let default_theme = resolve_default_theme(&mut themes, descriptor.default_theme.as_deref())?;
    let boards = parse_boards(&descriptor.boards)?;
    let commands = command_map(&boards)?;
    let release_repository = validate_release_repository(&descriptor.release_repository)?;
    let update = validate_product_update(descriptor.update.as_ref(), &release_repository)?;
    Ok(ValidatedProduct {
        id,
        binary_name,
        display_name,
        bundle_id,
        release_repository,
        icon_paths,
        credits: descriptor.credits.clone(),
        themes,
        default_theme,
        boards,
        commands,
        update,
    })
}

fn validate_product_update(
    source: Option<&UpdateSource>,
    release_repository: &str,
) -> Result<Option<UpdateSource>, ProductError> {
    source
        .map(|source| {
            let source = validate_update_source(source)?;
            require_release_repository(&source, release_repository)?;
            Ok(source)
        })
        .transpose()
}

/// Validates a declared known-product list. Duplicate product, binary or
/// bundle identities fail with the conflicting name.
pub fn validate_known_products(
    descriptors: &[ProductDescriptor],
) -> Result<Vec<ValidatedProduct>, ProductError> {
    let products = validate_each_product(descriptors)?;
    require_unique_product_ids(&products)?;
    require_unique_binaries(&products)?;
    require_unique_bundles(&products)?;
    Ok(products)
}

fn parse_asset_paths(values: &[String]) -> Result<Vec<String>, ProductError> {
    let mut paths = Vec::new();
    for value in values {
        paths.push(parse_asset_path(value)?);
    }
    Ok(paths)
}

fn parse_identities(values: &[String]) -> Result<Vec<Identity>, ProductError> {
    let mut identities = Vec::new();
    for value in values {
        let identity = parse_identity(value)?;
        if identities.contains(&identity) {
            return Err(ProductError::DuplicateIdentity {
                name: value.clone(),
            });
        }
        identities.push(identity);
    }
    Ok(identities)
}

fn resolve_default_theme(
    themes: &mut Vec<Identity>,
    default_theme: Option<&str>,
) -> Result<String, ProductError> {
    match default_theme {
        None => include_framework_default(themes),
        Some(name) => parse_theme_name(name),
    }
}

fn include_framework_default(themes: &mut Vec<Identity>) -> Result<String, ProductError> {
    let document = parse_identity(crate::DEFAULT_THEME_DOCUMENT)?;
    append_missing_theme(themes, &document);
    Ok(crate::DEFAULT_THEME.to_owned())
}

fn append_missing_theme(themes: &mut Vec<Identity>, theme: &Identity) {
    if !themes.iter().any(|existing| existing == theme) {
        themes.push(theme.clone());
    }
}

fn parse_theme_name(name: &str) -> Result<String, ProductError> {
    parse_display_name(name).map_err(|_| ProductError::UnknownTheme {
        name: name.to_string(),
    })
}

fn parse_boards(descriptors: &[BoardDescriptor]) -> Result<Vec<ValidatedBoard>, ProductError> {
    let mut boards = Vec::new();
    let mut ids = Vec::new();
    for descriptor in descriptors {
        let board = parse_board(descriptor)?;
        if ids.contains(&board.id) {
            return Err(ProductError::DuplicateIdentity {
                name: board.id.as_str().to_string(),
            });
        }
        ids.push(board.id.clone());
        boards.push(board);
    }
    Ok(boards)
}

fn parse_board(descriptor: &BoardDescriptor) -> Result<ValidatedBoard, ProductError> {
    Ok(ValidatedBoard {
        id: parse_identity(&descriptor.id)?,
        aliases: parse_identities(&descriptor.aliases)?,
        default_enabled: descriptor.default_enabled,
    })
}

fn command_map(boards: &[ValidatedBoard]) -> Result<BTreeMap<String, Identity>, ProductError> {
    let mut commands = BTreeMap::new();
    for board in boards {
        insert_command(&mut commands, board.id.as_str(), board.id.clone())?;
        for alias in &board.aliases {
            insert_command(&mut commands, alias.as_str(), board.id.clone())?;
        }
    }
    Ok(commands)
}

fn insert_command(
    commands: &mut BTreeMap<String, Identity>,
    name: &str,
    board_id: Identity,
) -> Result<(), ProductError> {
    if name == crate::MCP_COMMAND || name == crate::UPGRADE_COMMAND {
        Err(ProductError::ReservedCommand {
            name: name.to_string(),
        })
    } else if commands.contains_key(name) {
        Err(ProductError::DuplicateAlias {
            name: name.to_string(),
        })
    } else {
        commands.insert(name.to_string(), board_id);
        Ok(())
    }
}

fn validate_each_product(
    descriptors: &[ProductDescriptor],
) -> Result<Vec<ValidatedProduct>, ProductError> {
    let mut products = Vec::new();
    for descriptor in descriptors {
        products.push(validate_product(descriptor)?);
    }
    Ok(products)
}

fn require_unique_product_ids(products: &[ValidatedProduct]) -> Result<(), ProductError> {
    first_duplicate(products, |product| product.id.as_str())
        .map_or(Ok(()), |name| Err(ProductError::DuplicateIdentity { name }))
}

fn require_unique_binaries(products: &[ValidatedProduct]) -> Result<(), ProductError> {
    first_duplicate(products, |product| product.binary_name.as_str())
        .map_or(Ok(()), |name| Err(ProductError::DuplicateBinary { name }))
}

fn require_unique_bundles(products: &[ValidatedProduct]) -> Result<(), ProductError> {
    first_duplicate(products, |product| product.bundle_id.as_str())
        .map_or(Ok(()), |name| Err(ProductError::DuplicateBundle { name }))
}

fn first_duplicate(
    products: &[ValidatedProduct],
    field: fn(&ValidatedProduct) -> &str,
) -> Option<String> {
    let mut seen = Vec::new();
    for product in products {
        let name = field(product);
        if seen.iter().any(|existing| existing == name) {
            return Some(name.to_string());
        }
        seen.push(name.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{BoardDescriptor, ProductDescriptor, validate_known_products, validate_product};
    use crate::error::ProductError;

    fn board(id: &str) -> BoardDescriptor {
        BoardDescriptor {
            id: id.into(),
            aliases: Vec::new(),
            default_enabled: true,
        }
    }

    fn subspace_like() -> ProductDescriptor {
        ProductDescriptor {
            id: "subspace".into(),
            binary_name: "subspace".into(),
            display_name: "Subspace".into(),
            bundle_id: "com.example.subspace".into(),
            release_repository: "example-org/example-app".into(),
            icon_paths: vec!["assets/subspace.icns".into()],
            credits: Vec::new(),
            themes: vec!["one-dark".into()],
            default_theme: Some("Subspace One Dark".into()),
            boards: vec![
                board("actions"),
                board("github"),
                board("github-copilot-agents"),
                board("irc"),
                board("board-lab"),
            ],
            update: None,
        }
    }

    fn update_source() -> crate::UpdateSource {
        crate::UpdateSource {
            repository: "fixture/upstream".into(),
            tag_prefix: "tools/x/v".into(),
            archive_prefix: "x".into(),
            app_bundle: "X".into(),
            token_variable: "SUBSPACE_GITHUB_TOKEN".into(),
            channel: crate::Channel::Stable,
        }
    }

    #[test]
    fn validate_product_accepts_a_prefixed_multi_board_descriptor() {
        let product = validate_product(&subspace_like()).unwrap();
        assert_eq!(product.id.as_str(), "subspace");
        assert_eq!(product.binary_name.as_str(), "subspace");
        assert_eq!(product.display_name, "Subspace");
        assert_eq!(product.bundle_id, "com.example.subspace");
        assert_eq!(product.icon_paths, ["assets/subspace.icns".to_string()]);
        assert_eq!(product.themes[0].as_str(), "one-dark");
        assert!(product.boards[0].aliases.is_empty());
        assert_eq!(product.default_theme, "Subspace One Dark");
        assert_eq!(
            product.board_for_command("board-lab").unwrap().id.as_str(),
            "board-lab"
        );
        assert_eq!(product.boards.len(), 5);
        assert!(product.commands.contains_key("actions"));
    }

    #[test]
    fn board_default_enabled_is_propagated_without_normalization() {
        let mut descriptor = subspace_like();
        descriptor.boards[0].default_enabled = false;
        descriptor.boards[1].default_enabled = true;
        let product = validate_product(&descriptor).unwrap();
        assert!(!product.boards[0].default_enabled);
        assert!(product.boards[1].default_enabled);
    }

    #[test]
    fn duplicate_board_ids_return_the_conflicting_name() {
        let mut descriptor = subspace_like();
        descriptor.boards.push(board("actions"));
        let error = validate_product(&descriptor).unwrap_err();
        assert_eq!(
            error,
            ProductError::DuplicateIdentity {
                name: "actions".into()
            }
        );
    }

    #[test]
    fn duplicate_command_aliases_return_the_conflicting_name() {
        let mut descriptor = subspace_like();
        descriptor.boards[0].aliases.push("github".into());
        let error = validate_product(&descriptor).unwrap_err();
        assert_eq!(
            error,
            ProductError::DuplicateAlias {
                name: "github".into()
            }
        );
    }

    #[test]
    fn framework_root_command_names_are_reserved_for_every_product() {
        for name in ["mcp", "upgrade"] {
            let mut descriptor = subspace_like();
            descriptor.boards[0].aliases.push(name.into());
            let error = validate_product(&descriptor).unwrap_err();
            assert_eq!(error, ProductError::ReservedCommand { name: name.into() });
        }
    }

    #[test]
    fn release_repository_is_validated_even_without_updates() {
        let mut descriptor = subspace_like();
        descriptor.release_repository = "fixture/fork".into();
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.release_repository, "fixture/fork");
        assert!(product.update.is_none());
        for invalid in [
            "",
            "owner",
            "owner/name/extra",
            "owner/../name",
            "owner/ name",
        ] {
            descriptor.release_repository = invalid.into();
            let error = validate_product(&descriptor).unwrap_err();
            assert_eq!(
                error,
                ProductError::UnsafeReleaseRepository {
                    name: invalid.into()
                }
            );
            assert_eq!(
                error.to_string(),
                format!("unsafe release repository '{invalid}'")
            );
        }
    }

    #[test]
    fn update_repository_must_match_the_release_repository() {
        let mut descriptor = subspace_like();
        descriptor.release_repository = "fixture/fork".into();
        descriptor.update = Some(update_source());
        let error = validate_product(&descriptor).unwrap_err();
        assert_eq!(
            error,
            ProductError::UpdateRepositoryMismatch {
                repository: "fixture/upstream".into(),
                release_repository: "fixture/fork".into(),
            }
        );
        assert_eq!(
            error.to_string(),
            "update repository 'fixture/upstream' does not match release repository 'fixture/fork'"
        );
        descriptor.update.as_mut().unwrap().repository = "fixture/fork".into();
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.release_repository, "fixture/fork");
        assert_eq!(product.update.unwrap().repository, "fixture/fork");
    }

    #[test]
    fn an_unsafe_update_source_fails_product_validation() {
        let mut descriptor = subspace_like();
        let mut source = update_source();
        source.token_variable = "bad-name".into();
        descriptor.update = Some(source);
        let error = validate_product(&descriptor).unwrap_err();
        assert_eq!(
            error,
            ProductError::UnsafeUpdateSource {
                name: "bad-name".into()
            }
        );
        descriptor.update = None;
        assert_eq!(validate_product(&descriptor).unwrap().update, None);
    }

    #[test]
    fn unsafe_alias_returns_the_actual_value() {
        let mut descriptor = subspace_like();
        descriptor.boards[0].aliases.push("bad/name".into());
        let error = validate_product(&descriptor).unwrap_err();
        assert_eq!(
            error,
            ProductError::UnsafeIdentity {
                name: "bad/name".into()
            }
        );
    }

    #[test]
    fn blank_or_padded_default_theme_returns_the_theme_name() {
        for name in ["", " Cupertino Dark"] {
            let mut descriptor = subspace_like();
            descriptor.default_theme = Some(name.into());
            let error = validate_product(&descriptor).unwrap_err();
            assert_eq!(error, ProductError::UnknownTheme { name: name.into() });
        }
    }

    #[test]
    fn known_products_reject_duplicate_ids_and_binaries() {
        let first = subspace_like();
        let mut second = subspace_like();
        second.binary_name = "other".into();
        second.bundle_id = "com.example.other".into();
        let error = validate_known_products(&[first.clone(), second]).unwrap_err();
        assert_eq!(
            error,
            ProductError::DuplicateIdentity {
                name: "subspace".into()
            }
        );

        let mut other = subspace_like();
        other.id = "other".into();
        other.bundle_id = "com.example.other".into();
        let error = validate_known_products(&[first.clone(), other]).unwrap_err();
        assert_eq!(
            error,
            ProductError::DuplicateBinary {
                name: "subspace".into()
            }
        );

        let mut bundled = subspace_like();
        bundled.id = "other".into();
        bundled.binary_name = "other".into();
        let error = validate_known_products(&[first, bundled]).unwrap_err();
        assert_eq!(
            error,
            ProductError::DuplicateBundle {
                name: "com.example.subspace".into()
            }
        );
    }

    #[test]
    fn omitted_default_theme_uses_the_framework_theme_and_adds_its_document_when_missing() {
        let mut descriptor = subspace_like();
        descriptor.themes = vec!["solarized".into()];
        descriptor.default_theme = None;
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.default_theme, crate::DEFAULT_THEME);
        assert_eq!(
            product
                .themes
                .iter()
                .map(|theme| theme.as_str())
                .collect::<Vec<_>>(),
            ["solarized", crate::DEFAULT_THEME_DOCUMENT]
        );
    }

    #[test]
    fn omitted_default_theme_does_not_duplicate_an_existing_framework_document() {
        let mut descriptor = subspace_like();
        descriptor.themes = vec![crate::DEFAULT_THEME_DOCUMENT.into(), "solarized".into()];
        descriptor.default_theme = None;
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.default_theme, crate::DEFAULT_THEME);
        assert_eq!(
            product
                .themes
                .iter()
                .filter(|theme| theme.as_str() == crate::DEFAULT_THEME_DOCUMENT)
                .count(),
            1
        );
    }

    #[test]
    fn explicit_default_theme_is_preserved_without_adding_the_framework_theme() {
        let mut descriptor = subspace_like();
        descriptor.themes = vec!["one-dark".into(), "solarized".into()];
        descriptor.default_theme = Some("Solarized Dark".into());
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.default_theme, "Solarized Dark");
        assert_eq!(
            product
                .themes
                .iter()
                .map(|theme| theme.as_str())
                .collect::<Vec<_>>(),
            ["one-dark", "solarized"]
        );
    }

    #[test]
    fn apply_default_theme_override_selects_a_variant_by_name() {
        let mut descriptor = subspace_like();
        descriptor.themes = vec!["one-dark".into(), "solarized".into()];
        descriptor
            .apply_default_theme_override(Some("Solarized Dark"))
            .unwrap();
        assert_eq!(descriptor.default_theme.as_deref(), Some("Solarized Dark"));
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.default_theme, "Solarized Dark");
    }

    #[test]
    fn apply_default_theme_override_none_leaves_the_descriptor_unchanged() {
        let mut descriptor = subspace_like();
        descriptor.apply_default_theme_override(None).unwrap();
        assert_eq!(
            descriptor.default_theme.as_deref(),
            Some("Subspace One Dark")
        );
    }

    #[test]
    fn apply_default_theme_override_rejects_blank_and_padded_names() {
        let mut descriptor = subspace_like();
        for name in ["", "Solarized Dark "] {
            let error = descriptor
                .apply_default_theme_override(Some(name))
                .unwrap_err();
            assert_eq!(error, ProductError::UnknownTheme { name: name.into() });
            assert_eq!(
                descriptor.default_theme.as_deref(),
                Some("Subspace One Dark")
            );
        }
    }

    #[test]
    fn apply_default_theme_override_does_not_autoinclude_the_framework_document() {
        let mut descriptor = subspace_like();
        descriptor.themes = vec!["solarized".into()];
        descriptor
            .apply_default_theme_override(Some(crate::DEFAULT_THEME))
            .unwrap();
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.default_theme, crate::DEFAULT_THEME);
        assert_eq!(
            product
                .themes
                .iter()
                .map(|theme| theme.as_str())
                .collect::<Vec<_>>(),
            ["solarized"]
        );
    }
}
