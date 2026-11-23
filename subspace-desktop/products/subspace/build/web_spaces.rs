// The web Spaces built into Subspace. The build script lists each one's
// package files, with the rules `scripts/package-web-space.py` uses for a
// package, and writes `web_spaces.rs` into OUT_DIR, which the app includes.
// Like the package script, it fails on a link, a special file, or a name that
// is not portable or collides with another, which the app would otherwise
// fail to write at every start.

use std::path::{Path, PathBuf};

/// Folders a running web Space never needs, as in the package script.
const EXCLUDED_DIRECTORIES: &[&str] = &["test", "tests", "__tests__", "node_modules"];
/// Development files left out of a package, as in the package script.
const EXCLUDED_FILES: &[&str] = &[
    "mise.toml",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "tsconfig.json",
];
/// Windows device names, refused as a name's stem, as in the package script.
const DEVICES: &[&str] = &["CON", "PRN", "AUX", "NUL"];
const MAX_NAME_BYTES: usize = 255;
const EXCLUDED_SUFFIXES: &[&str] = &[
    ".test.js",
    ".test.mjs",
    ".test.ts",
    ".spec.js",
    ".spec.mjs",
    ".spec.ts",
];

/// Whether the package keeps a file or folder named `name`.
pub fn packaged(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') {
        return false;
    }
    if is_dir {
        return !EXCLUDED_DIRECTORIES.contains(&name);
    }
    !EXCLUDED_FILES.contains(&name) && !EXCLUDED_SUFFIXES.iter().any(|end| name.ends_with(end))
}

/// Whether `name` is a portable file or folder name, by the package
/// script's rule: printable ASCII from a small set, no `.git`, no trailing
/// dot or space and no Windows device name.
pub fn portable(name: &str) -> bool {
    !reserved(name) && name.chars().all(allowed) && !device(name)
}

/// Whether `c` may appear in a portable name.
fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || " -_.@()+,[]{}".contains(c)
}

/// Whether `name` is empty, `.`, `..`, `.git`, too long, or ends in a dot
/// or a space.
fn reserved(name: &str) -> bool {
    matches!(name, "" | "." | "..")
        || name.eq_ignore_ascii_case(".git")
        || name.ends_with(['.', ' '])
        || name.len() > MAX_NAME_BYTES
}

/// Whether `name`'s stem, before its first dot, is a Windows device name.
fn device(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let numbered = stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.ends_with(|c: char| c.is_ascii_digit());
    numbered || DEVICES.contains(&stem.as_str())
}

/// The package files under `root`, as sorted `/`-separated relative paths.
/// An error names a link, a special file, or a name that is not portable or
/// differs from another only in case.
pub fn package_files(root: &Path) -> std::io::Result<Vec<String>> {
    let mut found = Vec::new();
    collect(root, "", &mut found)?;
    found.sort();
    let mut seen = std::collections::HashSet::new();
    if let Some(twice) = found.iter().find(|name| !seen.insert(name.to_lowercase())) {
        return Err(refused(
            twice,
            "differs from another package file only in case",
        ));
    }
    Ok(found)
}

fn collect(folder: &Path, prefix: &str, found: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !packaged(&name, kind.is_dir()) {
            continue;
        }
        let relative = format!("{prefix}{name}");
        if !portable(&name) {
            return Err(refused(&relative, "is not a portable name"));
        }
        if kind.is_dir() {
            collect(&entry.path(), &format!("{relative}/"), found)?;
        } else if kind.is_file() {
            found.push(relative);
        } else {
            return Err(refused(&relative, "is a link or a special file"));
        }
    }
    Ok(())
}

fn refused(relative: &str, why: &str) -> std::io::Error {
    std::io::Error::other(format!("The built-in web Space file {relative} {why}."))
}

/// Rust source for `const <name>: &[(&str, &[u8])]` holding `files` from
/// `root`.
pub fn constant(name: &str, root: &Path, files: &[String]) -> String {
    let entries: String = files
        .iter()
        .map(|relative| {
            let path: PathBuf = relative
                .split('/')
                .fold(root.to_owned(), |path, part| path.join(part));
            format!(
                "    ({relative:?}, include_bytes!({:?})),\n",
                path.to_string_lossy()
            )
        })
        .collect();
    format!("pub(crate) const {name}: &[(&str, &[u8])] = &[\n{entries}];\n")
}

/// Writes `web_spaces.rs` into `output` for the Fleet Console in
/// `workspace/web-spaces/fleet-console` and asks Cargo to rebuild when it
/// changes.
pub fn embed(workspace: &Path, output: &Path) -> std::io::Result<()> {
    let root = workspace.join("web-spaces/fleet-console").canonicalize()?;
    let files = package_files(&root)?;
    println!("cargo:rerun-if-changed={}", root.display());
    std::fs::write(
        output.join("web_spaces.rs"),
        constant("FLEET_CONSOLE_FILES", &root, &files),
    )
}
