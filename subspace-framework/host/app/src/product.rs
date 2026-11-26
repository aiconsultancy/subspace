//! Product configuration paths and headless alias validation.

use std::path::PathBuf;

use gpui_kit::{App, Global};
use serde_json::Value;
use subspace_dashboard::CurrentProduct;
use subspace_product::{
    OsKind, PlatformEnv, ProductContext, ProductDescriptor, product_data_name, validate_product,
};

pub(crate) struct ProductFiles {
    pub config: PathBuf,
    pub state: PathBuf,
}

impl Global for ProductFiles {}

/// Presentation name, separate from the name that identifies release bundles.
pub(crate) struct DesktopName(String);

impl Global for DesktopName {}

impl DesktopName {
    pub(crate) fn new(name: &str, prefix: Option<&str>) -> Self {
        Self(
            prefix
                .filter(|prefix| !prefix.is_empty())
                .map_or_else(|| name.to_owned(), |prefix| format!("{prefix} {name}")),
        )
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn desktop_name(cx: &App) -> &str {
    if let Some(name) = cx.try_global::<DesktopName>() {
        return name.as_str();
    }
    cx.try_global::<CurrentProduct>()
        .map_or("Subspace", |context| &context.0.product.display_name)
}

/// The app's identity, its product context and its local diagnostics.
pub(crate) fn install_context(
    context: std::sync::Arc<ProductContext>,
    prefix: Option<&str>,
    cx: &mut App,
) {
    let name = DesktopName::new(&context.product.display_name, prefix);
    cx.set_app_identity(&context.product.bundle_id, name.as_str());
    cx.set_global(name);
    let diagnostics = crate::diagnostics::local_copy(&context);
    cx.set_global(subspace_dashboard::copy_details::CopyDiagnostics::new(
        diagnostics,
    ));
    cx.set_global(CurrentProduct(context));
}

impl ProductFiles {
    pub(crate) fn for_context(context: &ProductContext) -> Self {
        Self {
            config: config_file(context),
            state: preference_file(context),
        }
    }
}

/// Existing cross-platform config first, then native config. New settings use
/// the native root when available, preserving Subspace's existing location.
pub fn config_file(context: &ProductContext) -> PathBuf {
    let roots = context.product_roots();
    roots
        .product_roots()
        .into_iter()
        .map(|root| root.join("config.json"))
        .find(|file| file.exists())
        .unwrap_or_else(|| {
            roots
                .native
                .unwrap_or(roots.cross_platform)
                .join("config.json")
        })
}

fn preference_file(context: &ProductContext) -> PathBuf {
    state_directory(&context.env).join("state.json")
}

/// The app's data directory, [`product_data_name`] in the platform state
/// directory, which holds `state.json`.
pub(crate) fn state_directory(env: &PlatformEnv) -> PathBuf {
    let base = match env.os {
        OsKind::Linux => env
            .xdg_state_home
            .clone()
            .unwrap_or_else(|| env.home.join(".local/state")),
        OsKind::Macos => env.home.join("Library/Application Support"),
        OsKind::Windows => env
            .local_app_data
            .clone()
            .unwrap_or_else(|| env.home.join("AppData/Local")),
    };
    base.join(product_data_name())
}

/// Loads only product command aliases before CLI or desktop startup. Service
/// credentials are not read here. It first names the data directory after
/// the binary, as `launch` does, so nothing is read under another app's
/// name; it fails, reading and writing nothing, when a data path was read
/// under another name before.
pub fn prepare_context(
    mut descriptor: ProductDescriptor,
    version: &str,
) -> Result<ProductContext, String> {
    let product = validate_product(&descriptor).map_err(|error| error.to_string())?;
    crate::startup::name_data_directory(product.binary_name.as_str())?;
    let env = PlatformEnv::from_process().map_err(|error| error.to_string())?;
    let mut context = context_with_env(&mut descriptor, env)?;
    context.version = version.to_owned();
    Ok(context)
}

fn context_with_env(
    descriptor: &mut ProductDescriptor,
    env: PlatformEnv,
) -> Result<ProductContext, String> {
    let validated = validate_product(descriptor).map_err(|error| error.to_string())?;
    let context = ProductContext::new(validated, env);
    let file = config_file(&context);
    if let Some(value) = load_alias_file(&file)? {
        apply_aliases(descriptor, &value)?;
    }
    let product = validate_product(descriptor).map_err(|error| error.to_string())?;
    Ok(ProductContext::new(product, context.env))
}

fn load_alias_file(file: &std::path::Path) -> Result<Option<Value>, String> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot read product configuration {}: {error}",
                file.display()
            ));
        }
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| format!("invalid product configuration JSON: {}", file.display()))
}

fn apply_aliases(descriptor: &mut ProductDescriptor, value: &Value) -> Result<(), String> {
    let Some(aliases) = value.pointer("/commands/aliases") else {
        return Ok(());
    };
    let aliases = aliases
        .as_object()
        .ok_or("commands.aliases must be an object")?;
    for (id, value) in aliases {
        let board = descriptor
            .boards
            .iter_mut()
            .find(|board| &board.id == id)
            .ok_or_else(|| format!("commands.aliases names an excluded board: {id}"))?;
        let values = value
            .as_array()
            .ok_or_else(|| format!("commands.aliases.{id} must be an array"))?;
        for value in values {
            let alias = value
                .as_str()
                .ok_or_else(|| format!("commands.aliases.{id} must contain strings"))?;
            board.aliases.push(alias.to_owned());
        }
    }
    Ok(())
}

pub(crate) fn gui_config_file(cx: &App) -> PathBuf {
    cx.global::<ProductFiles>().config.clone()
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopName, ProductFiles, apply_aliases, config_file, context_with_env, load_alias_file,
        preference_file,
    };
    use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture, validate_product};

    #[test]
    fn desktop_name_can_be_prefixed_without_changing_the_bundle_name() {
        let name = "Subspace";
        assert_eq!(
            DesktopName::new(name, Some("Example")).as_str(),
            "Example Subspace"
        );
        assert_eq!(DesktopName::new(name, Some("")).as_str(), name);
        assert_eq!(DesktopName::new(name, None).as_str(), name);
    }

    #[test]
    fn product_aliases_validate_collisions_and_excluded_boards() {
        let mut descriptor = fixture::descriptor();
        apply_aliases(
            &mut descriptor,
            &serde_json::json!({"commands":{"aliases":{"github":["gh"]}}}),
        )
        .unwrap();
        assert!(
            validate_product(&descriptor)
                .unwrap()
                .board_for_command("gh")
                .is_some()
        );
        apply_aliases(
            &mut descriptor,
            &serde_json::json!({"commands":{"aliases":{"github":["github"]}}}),
        )
        .unwrap();
        assert!(validate_product(&descriptor).is_err());
        assert!(
            apply_aliases(
                &mut descriptor,
                &serde_json::json!({"commands":{"aliases":{"missing":["m"]}}})
            )
            .unwrap_err()
            .contains("excluded board")
        );
    }

    #[test]
    fn gui_files_are_product_specific() {
        let root = tempfile::tempdir().unwrap();
        let known = fixture::products().unwrap();
        let env = PlatformEnv {
            os: OsKind::Macos,
            home: root.path().to_owned(),
            xdg_config_home: None,
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        };
        let context = ProductContext::new(known[0].clone(), env.clone());
        let files = ProductFiles::for_context(&context);
        assert_eq!(files.state, preference_file(&context));
        assert_eq!(files.config, config_file(&context));
        let name = subspace_product::product_data_name();
        assert!(
            preference_file(&context).ends_with(format!("Application Support/{name}/state.json"))
        );
        assert!(config_file(&context).ends_with(format!("Application Support/{name}/config.json")));
        assert!(!root.path().join("Library").exists());
        let mut descriptor = fixture::descriptor();
        let reloaded = context_with_env(&mut descriptor, env).unwrap();
        assert_eq!(reloaded.product.id.as_str(), "subspace");
        assert!(!root.path().join("Library").exists());
    }

    #[test]
    fn alias_loader_does_not_repeat_configuration_contents() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("config.json");
        assert_eq!(load_alias_file(&file).unwrap(), None);
        std::fs::write(&file, "a-secret-that-is-not-json").unwrap();
        let error = load_alias_file(&file).unwrap_err();
        assert!(error.contains("invalid product configuration JSON"));
        assert!(!error.contains("a-secret"));
        std::fs::write(&file, "{}").unwrap();
        assert_eq!(load_alias_file(&file).unwrap(), Some(serde_json::json!({})));
    }
}
