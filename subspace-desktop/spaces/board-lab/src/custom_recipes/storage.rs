//! Immutable exclusive files under `custom-recipes/<id>/<hash>.json`.

use super::CustomRecipe;
use super::definition::{MAX_IDS, MAX_RECORD, MAX_REVS, check_id, fingerprint_recipe, is_hash};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn store(root: &Path, recipe: &CustomRecipe) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(recipe).context("serializing custom recipe")?;
    bound_record(&bytes)?;
    let base = root.join("custom-recipes");
    let dest = revision_file(&base, &recipe.id, &recipe.hash);
    if meta(&dest)?.is_some() {
        same(&dest, &bytes)?;
        return Ok(());
    }
    enforce_capacity(&base, &recipe.id)?;
    ensure_dir(&base)?;
    ensure_dir(&base.join(&recipe.id))?;
    write_exclusive(&dest, &bytes)
}

pub(super) fn list(root: &Path) -> Result<Vec<CustomRecipe>> {
    let mut recipes = Vec::new();
    for (id, hash, path) in scan(&root.join("custom-recipes"))?.1 {
        recipes.push(load_file(&path, &id, &hash)?);
    }
    recipes.sort_by(|left, right| left.id.cmp(&right.id).then(left.hash.cmp(&right.hash)));
    Ok(recipes)
}

pub(super) fn load(root: &Path, id: &str, hash: &str) -> Result<CustomRecipe> {
    meta(&root.join("custom-recipes"))?;
    let path = revision_file(&root.join("custom-recipes"), id, hash);
    if let Some(parent) = path.parent() {
        meta(parent)?;
    }
    load_file(&path, id, hash)
}

fn enforce_capacity(base: &Path, id: &str) -> Result<()> {
    let (ids, revs) = scan(base)?;
    if !base.join(id).exists() && ids >= MAX_IDS {
        bail!("at most {MAX_IDS} custom recipe ids can be stored");
    }
    if revs.len() >= MAX_REVS {
        bail!("at most {MAX_REVS} custom recipe revisions can be stored");
    }
    Ok(())
}

fn load_file(path: &Path, id: &str, hash: &str) -> Result<CustomRecipe> {
    meta(path)?;
    let bytes = fs::read(path).with_context(|| io_msg("reading", path))?;
    bound_record(&bytes)?;
    let recipe: CustomRecipe =
        serde_json::from_slice(&bytes).with_context(|| io_msg("parsing", path))?;
    let expected = fingerprint_recipe(&recipe)?;
    if recipe.id != id || recipe.hash != hash || expected != recipe.hash {
        bail!("recipe hash does not match contents");
    }
    Ok(recipe)
}

fn write_exclusive(path: &Path, bytes: &[u8]) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_exclusive(path, bytes),
        Err(error) => Err(error).with_context(|| io_msg("stat", path)),
        Ok(meta) if meta.file_type().is_symlink() => Err(symlink_rejected(path)),
        Ok(_) => compare_bytes(path, bytes),
    }
}

fn create_exclusive(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().context("Recipe path has no directory")?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    match file.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => same(path, bytes),
        Err(error) => Err(error.error).with_context(|| io_msg("storing", path)),
    }
}

fn compare_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let existing = fs::read(path).with_context(|| io_msg("reading", path))?;
    if existing == bytes {
        Ok(())
    } else {
        bail!("immutable recipe revision already exists with different contents")
    }
}

fn same(path: &Path, bytes: &[u8]) -> Result<()> {
    meta(path)?;
    compare_bytes(path, bytes)
}

fn scan(base: &Path) -> Result<(usize, Vec<(String, String, PathBuf)>)> {
    let Some(info) = meta(base)? else {
        return Ok((0, Vec::new()));
    };
    if !info.is_dir() {
        bail!("custom-recipes is not a directory")
    }
    collect_revisions(base)
}

fn collect_revisions(base: &Path) -> Result<(usize, Vec<(String, String, PathBuf)>)> {
    let mut ids = 0;
    let mut found = Vec::new();
    for id_ent in fs::read_dir(base).with_context(|| io_msg("listing", base))? {
        let id_ent = id_ent?;
        meta(&id_ent.path())?;
        if !id_ent.file_type()?.is_dir() {
            continue;
        }
        ids += 1;
        let Some(id) = id_ent.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if check_id(&id).is_err() {
            continue;
        }
        push_id_files(&id, &id_ent.path(), &mut found)?;
    }
    Ok((ids, found))
}

fn push_id_files(
    id: &str,
    directory: &Path,
    found: &mut Vec<(String, String, PathBuf)>,
) -> Result<()> {
    for file in fs::read_dir(directory)? {
        let file = file?;
        let path = file.path();
        meta(&path)?;
        if let Some(hash) = file.file_name().to_str().and_then(json_hash) {
            found.push((id.to_owned(), hash.to_owned(), path));
        }
    }
    Ok(())
}

fn ensure_dir(path: &Path) -> Result<()> {
    if let Some(info) = meta(path)? {
        if info.is_dir() {
            return Ok(());
        }
        bail!("{} is not a directory", path.display());
    }
    fs::create_dir_all(path).with_context(|| io_msg("creating", path))?;
    meta(path).map(|_| ())
}

fn meta(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| io_msg("stat", path)),
        Ok(meta) if meta.file_type().is_symlink() => Err(symlink_rejected(path)),
        Ok(meta) => Ok(Some(meta)),
    }
}

fn json_hash(name: &str) -> Option<&str> {
    name.strip_suffix(".json").filter(|hash| is_hash(hash))
}

fn revision_file(base: &Path, id: &str, hash: &str) -> PathBuf {
    base.join(id).join(format!("{hash}.json"))
}

fn io_msg(action: &str, path: &Path) -> String {
    format!("{action} {}", path.display())
}

fn bound_record(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_RECORD {
        bail!("recipe record exceeds the {MAX_RECORD} byte limit");
    }
    Ok(())
}

fn symlink_rejected(path: &Path) -> anyhow::Error {
    anyhow::anyhow!("symlink rejected: {}", path.display())
}
