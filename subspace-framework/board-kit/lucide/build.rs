//! Generates one `SvgIcon` constant per Lucide SVG in GPUI Kit's catalog.
//! Ported from gupi's `gpui-lucide` (Apache-2.0,
//! https://github.com/suxiaoshao/gupi/tree/9fb1bf1a298e66425b691d74df6ec4a5cbb9c19e/crates/gpui-lucide).
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

const ICONS_DIR: &str = "DEP_GPUI_KIT_DEFAULT_ICONS_ICONS_DIR";

// qual:allow(complexity, error_handling) reason: "A build script reports failure by panicking; each expect names the missing build input."
fn main() {
    let directory = PathBuf::from(
        env::var_os(ICONS_DIR).expect("gpui-kit-assets must provide its icons-dir metadata"),
    );
    println!("cargo:rerun-if-env-changed={ICONS_DIR}");
    println!("cargo:rerun-if-changed={}", directory.display());
    println!("cargo:rerun-if-changed=build.rs");

    let paths: BTreeSet<PathBuf> = fs::read_dir(&directory)
        .expect("read the GPUI Kit icon directory")
        .map(|entry| entry.expect("read an icon entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .collect();
    assert!(!paths.is_empty(), "the GPUI Kit icon catalog is empty");

    let mut names = BTreeSet::new();
    let mut code = String::from("#[allow(non_upper_case_globals)]\nimpl SvgIcon {\n");
    for path in &paths {
        let (slug, name) = constant_name(path);
        assert!(names.insert(name.clone()), "duplicate icon name: {name}");
        let file = path.to_str().expect("UTF-8 icon path");
        writeln!(code, "    /// Lucide `{slug}`.").expect("write to a String");
        writeln!(
            code,
            "    pub const {name}: Self = Self::new(include_bytes!({file:?}));"
        )
        .expect("write to a String");
    }
    code.push_str("}\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    fs::write(output.join("icons.rs"), code).expect("write the icon constants");
}

/// `arrow-up.svg` becomes (`arrow-up`, `ArrowUp`).
fn constant_name(path: &Path) -> (String, String) {
    let slug = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("UTF-8 icon name")
        .to_owned();
    let name: String = slug
        .split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            first.into_iter().chain(chars).collect::<String>()
        })
        .collect();
    assert!(
        name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name.chars().all(|c| c.is_ascii_alphanumeric()),
        "icon {slug} does not make a valid Rust name"
    );
    (slug, name)
}
