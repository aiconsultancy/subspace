//! A product's theme for a new install, recorded as the saved theme at the
//! desktop app's first start. Once recorded it is the user's choice like any
//! other, so a later change of the product's default never moves it, and an
//! install with no recorded theme keeps showing the product's default.

use std::io;
use std::path::Path;

use subspace_core::state::State;
use subspace_product::{ProductContext, ValidatedProduct};

use super::THEME_PREF;
use super::loader::ProductThemes;
use crate::product::ProductFiles;

/// Records `theme` as the saved theme when `context`'s product has neither a
/// config file nor a state file: a new install. Returns whether it recorded.
/// `product_themes` are the app's own theme documents
/// (`ProductBuild.product_themes`, `&[]` for none), which `theme` may be a
/// variant of. An existing install, including one whose files are unreadable
/// or malformed, is left alone. Nothing is written, and an error returned,
/// when a new install's product themes fail the checks startup makes, when
/// `theme` is not a variant of the product's theme catalogue, or when the
/// files' folder cannot be read, so a new install cannot be told from an
/// existing one. It first names the data directory after the context's
/// binary, as `prepare_context` does, and returns that error, writing
/// nothing, when a data path was read under another name before. Only the
/// desktop app calls this, before it starts; command-line and headless runs
/// must not.
// qual:api (apps call it from their entrypoint, before the desktop starts)
pub fn record_new_install_theme(
    context: &ProductContext,
    theme: &str,
    product_themes: ProductThemes,
) -> Result<bool, String> {
    crate::startup::name_data_directory(context.product.binary_name.as_str())?;
    let files = ProductFiles::for_context(context);
    let new = is_new_install(&files).map_err(|error| file_error(&files, &error))?;
    new.then(|| record(&context.product, &files, theme, product_themes))
        .transpose()
        .map(|recorded| recorded.is_some())
}

/// Saves `theme`, a variant of `product`'s catalogue, into a new state file,
/// once `documents` pass startup's checks.
fn record(
    product: &ValidatedProduct,
    files: &ProductFiles,
    theme: &str,
    documents: ProductThemes,
) -> Result<(), String> {
    super::loader::validate_product_themes(product, documents)?;
    super::loader::require_catalogue_variant(product, "new install theme", theme, documents)?;
    save_theme(&files.state, theme).map_err(|error| file_error(files, &error))
}

fn save_theme(file: &Path, theme: &str) -> io::Result<()> {
    let mut state = State::default();
    state.set(THEME_PREF, theme);
    state.save(file)
}

fn file_error(files: &ProductFiles, error: &io::Error) -> String {
    format!(
        "could not record the new install's theme in {}: {error}",
        files.state.display()
    )
}

/// True when neither the config file nor the state file exists. Both are
/// checked without following links, so a dangling link counts as a file.
fn is_new_install(files: &ProductFiles) -> io::Result<bool> {
    Ok(absent(&files.config)? && absent(&files.state)?)
}

/// Whether nothing is at `path`. An error other than "not found", such as an
/// unreadable folder, is returned rather than guessed at.
fn absent(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}
