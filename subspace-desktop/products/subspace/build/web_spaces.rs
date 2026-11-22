// The web Spaces built into Subspace. The build script lists each one's
// package files, with the rules `scripts/package-web-space.py` uses for a
// package, and writes `web_spaces.rs` into OUT_DIR, which the app includes.

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

/// The package files under `root`, as sorted `/`-separated relative paths.
pub fn package_files(root: &Path) -> std::io::Result<Vec<String>> {
    let mut found = Vec::new();
    collect(root, "", &mut found)?;
    found.sort();
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
        if kind.is_dir() {
            collect(&entry.path(), &format!("{relative}/"), found)?;
        } else if kind.is_file() {
            found.push(relative);
        }
    }
    Ok(())
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
