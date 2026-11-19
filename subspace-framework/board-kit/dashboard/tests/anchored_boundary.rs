//! Keep menu placement and low-level host construction in the shared boundary,
//! across the Framework's own crates. Apps check their Spaces themselves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const HOST_CALLS: &[&str] = &[
    "anchored()",
    ".snap_to_window",
    "Positioner::",
    "Popup::new(",
    "Popover::new(",
    "PopupMenu::build(",
    "Select::new(",
    ".dropdown_menu(",
    ".dropdown_menu_with_anchor(",
    ".context_menu(",
];

fn calls(source: &str) -> usize {
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    HOST_CALLS
        .iter()
        .map(|pattern| compact.matches(pattern).count())
        .sum()
}

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if name == "tests" || name == "runtime_tests" || name.ends_with("_tests.rs") {
            continue;
        }
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") && name != "tests.rs" {
            files.push(path);
        }
    }
}

fn package_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let package = entry.unwrap().path();
        let source = package.join("src");
        if package.join("Cargo.toml").is_file() && source.is_dir() {
            source_files(&source, files);
        }
    }
}

fn production_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for family in [
        "spaces",
        "host",
        "board-kit",
        "services",
        "runtimes",
        "clients",
    ] {
        package_files(&root.join(family), &mut files);
    }
    for board in std::fs::read_dir(root.join("spaces")).unwrap() {
        let owned = board.unwrap().path().join("crates");
        if owned.is_dir() {
            package_files(&owned, &mut files);
        }
    }
    files
}

fn outside_boundary(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap();
    relative != Path::new("board-kit/dashboard/src/anchored.rs")
        && !relative.starts_with("board-kit/dashboard/src/anchored")
}

#[test]
fn native_menu_hosts_and_positions_stay_in_the_shared_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let actual: BTreeMap<_, _> = production_files(&root)
        .into_iter()
        .filter(|path| outside_boundary(&root, path))
        .filter_map(|path| {
            let count = calls(&std::fs::read_to_string(&path).unwrap());
            let name = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (count != 0).then_some((name, count))
        })
        .collect();
    assert!(
        actual.is_empty(),
        "Route menu hosts and placement through dashboard::anchored: {actual:?}"
    );
}

#[test]
fn the_boundary_check_detects_direct_hosts_despite_whitespace() {
    assert_eq!(calls("Select :: new ( &state )"), 1);
    assert_eq!(calls("Popover::new(\"bad\").dropdown_menu(|menu| menu)"), 2);
    assert_eq!(
        calls("anchored :: select(&state).accessibility_label(\"Theme\")"),
        0
    );
}

#[test]
fn anchor_builder_seam_does_not_exempt_native_hosts_or_placement() {
    assert_eq!(
        calls("host.show_menu_with(window, cx, |menu, _, _| menu.item(item))"),
        0
    );
    assert_eq!(
        calls("let menu = PopupMenu::build(window, cx, build); host.show_menu(menu, window, cx);"),
        1,
        "passing a separately built native menu to Anchor is not a construction exemption"
    );
    assert_eq!(
        calls(
            "host.show_menu_with(window, cx, |menu, _, _| { Popover::new(\"bad\"); menu.snap_to_window() })"
        ),
        2,
        "the shared seam does not whitelist arbitrary native hosts or positions inside a builder"
    );
}

#[test]
fn board_owned_source_roots_are_discovered_from_package_manifests() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for family in [
        "spaces",
        "host",
        "board-kit",
        "services",
        "runtimes",
        "clients",
    ] {
        std::fs::create_dir_all(root.join(family)).unwrap();
    }
    let owned = root.join("spaces/fixture/crates/support");
    std::fs::create_dir_all(owned.join("src")).unwrap();
    std::fs::write(owned.join("Cargo.toml"), "[package]\nname = 'support'\n").unwrap();
    let source = owned.join("src/lib.rs");
    std::fs::write(&source, "Popover::new(\"bad\")").unwrap();
    let unowned = root.join("spaces/fixture/crates/not-a-package/src");
    std::fs::create_dir_all(&unowned).unwrap();
    std::fs::write(unowned.join("lib.rs"), "Popover::new(\"also bad\")").unwrap();
    assert_eq!(production_files(root), [source]);
}
