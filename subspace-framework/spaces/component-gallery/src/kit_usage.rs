//! Which GPUI Kit components the Framework uses, read from its source outside
//! the gallery. Apps built on the Framework are not scanned. The catalogue records the answer in [`Kit::used`]; the test below
//! fails when the record and the source disagree, so the gallery's
//! "Hide unused GPUI Kit components" toggle stays accurate.
//!
//! A file uses a component when it imports or names one of the component's
//! paths below `gpui_kit::component` (or `gpui_kit::base`, its behaviour
//! layer), or, for a path that contains `(`, such
//! as `.open_sheet(` or `img(`, contains that text. `use` trees with braces, `self`, globs and `as` are
//! expanded. Test and example code does not count: `tests` and `examples`
//! folders, `tests.rs` and `*_tests.rs` files, and `#[cfg(test)]` inline
//! modules.

use std::path::{Path, PathBuf};

use crate::catalog::{Item, Kit, items};

/// The folders that hold the Framework's own Rust code, relative to its root.
const ROOTS: [&str; 7] = [
    "services",
    "board-kit",
    "runtimes",
    "host",
    "clients",
    "spaces",
    "src",
];
const GALLERY: &str = "spaces/component-gallery";
const SKIPPED_FOLDERS: [&str; 5] = ["target", "tests", "examples", "benches", "node_modules"];
const TEST_MARK: &str = "#[cfg(test)]";
const KIT: &str = "gpui_kit::";
const LAYERS: [&str; 2] = ["component", "base"];

/// One source file, reduced to the text that counts and its Kit paths.
struct Source {
    path: PathBuf,
    text: String,
    paths: Vec<Vec<String>>,
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
}

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() && !SKIPPED_FOLDERS.contains(&name) && !path.ends_with(GALLERY) {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_file(&path) {
            files.push(path);
        }
    }
}

/// The production text of a file: no comment lines and no inline tests.
fn production_text(text: &str) -> String {
    let text: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    without_test_modules(&text.join("\n"))
}

/// `text` without its `#[cfg(test)]` inline modules, matched by braces.
fn without_test_modules(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(TEST_MARK) {
        kept.push_str(&rest[..at]);
        let after = &rest[at + TEST_MARK.len()..];
        let Some(open) = inline_module(after) else {
            rest = after;
            continue;
        };
        rest = &after[closing_brace(after, open)..];
    }
    kept.push_str(rest);
    kept
}

/// The offset of the `{` of an inline `mod name {` right after a test mark.
fn inline_module(text: &str) -> Option<usize> {
    let item = text.trim_start();
    let skipped = text.len() - item.len();
    let head = item.split_once('{')?.0;
    (item.starts_with("mod ") && !head.contains(';')).then(|| skipped + head.len())
}

/// The offset just after the brace that closes the one at `open`.
fn closing_brace(text: &str, open: usize) -> usize {
    let mut depth = 0usize;
    for (at, char) in text[open..].char_indices() {
        match char {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => continue,
        }
        if depth == 0 {
            return open + at + 1;
        }
    }
    text.len()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Separator,
    Open,
    Close,
    Comma,
    Glob,
}

/// The text of the path that starts at `text`: up to the first character
/// that cannot continue it, or the brace that closes its outermost group.
fn path_extent(text: &str) -> &str {
    let mut depth = 0usize;
    for (at, char) in text.char_indices() {
        let continues = match char {
            '{' => {
                depth += 1;
                true
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return &text[..=at];
                }
                true
            }
            ',' => depth > 0,
            char if char.is_whitespace() => depth > 0,
            char => char.is_alphanumeric() || matches!(char, '_' | ':' | '*'),
        };
        if !continues {
            return &text[..at];
        }
    }
    text
}

/// The tokens of a path's text.
fn tokens_of(extent: &str) -> Vec<Token> {
    let spaced = extent
        .replace("::", " :: ")
        .replace('{', " { ")
        .replace('}', " } ")
        .replace(',', " , ")
        .replace('*', " * ");
    spaced
        .split_whitespace()
        .map(|word| match word {
            "::" => Token::Separator,
            "{" => Token::Open,
            "}" => Token::Close,
            "," => Token::Comma,
            "*" => Token::Glob,
            word => Token::Word(word.to_owned()),
        })
        .collect()
}

/// The tokens of the path that starts at `text`.
fn path_tokens(text: &str) -> Vec<Token> {
    tokens_of(path_extent(text))
}

/// Expands one use tree into full paths, each a list of segments.
fn expand(tokens: &[Token], prefix: &[String], paths: &mut Vec<Vec<String>>) -> usize {
    let mut path = prefix.to_vec();
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        match token {
            Token::Word(word) if word == "as" => at += 1,
            Token::Word(word) if word == "self" => {}
            Token::Word(word) => path.push(word.clone()),
            Token::Separator => {}
            Token::Glob => path.push("*".to_owned()),
            Token::Open => {
                at += expand_group(&tokens[at + 1..], &path, paths) + 1;
                return at + 1;
            }
            Token::Comma | Token::Close => break,
        }
        at += 1;
    }
    paths.push(path);
    at
}

/// Expands the trees of one brace group; returns the tokens it read.
fn expand_group(tokens: &[Token], prefix: &[String], paths: &mut Vec<Vec<String>>) -> usize {
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        match token {
            Token::Close => return at,
            Token::Comma => at += 1,
            _ => at += expand(&tokens[at..], prefix, paths),
        }
    }
    at
}

/// Every Kit path in `text`, relative to `gpui_kit::component` or to
/// `gpui_kit::base`, whose behaviour layer the components re-export.
fn component_paths(text: &str) -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    for (at, _) in text.match_indices(KIT) {
        expand(&path_tokens(&text[at + KIT.len()..]), &[], &mut paths);
    }
    paths
        .into_iter()
        .filter_map(|path| {
            LAYERS
                .contains(&path.first()?.as_str())
                .then(|| path[1..].to_vec())
        })
        .filter(|path| !path.is_empty())
        .collect()
}

fn sources() -> Vec<Source> {
    let root = project_root();
    let mut files = Vec::new();
    for folder in ROOTS {
        rust_files(&root.join(folder), &mut files);
    }
    files
        .into_iter()
        .filter_map(|path| {
            let text = production_text(&std::fs::read_to_string(&path).ok()?);
            let paths = component_paths(&text);
            Some(Source { path, text, paths })
        })
        .collect()
}

/// Whether `path`, imported by a file with `text`, uses the component path `needle`.
fn path_uses(path: &[String], needle: &[&str], text: &str) -> bool {
    if path.last().is_some_and(|last| last == "*") {
        let module = &path[..path.len() - 1];
        return needle.len() > module.len() && module.iter().zip(needle).all(|(a, b)| a == b);
    }
    if path.len() >= needle.len() {
        return path.iter().zip(needle).all(|(a, b)| a == b);
    }
    let Some(last) = path.last() else {
        return false;
    };
    path.iter().zip(needle).all(|(a, b)| a == b)
        && text.contains(&format!("{last}::{}", needle[path.len()..].join("::")))
}

fn source_uses(source: &Source, needle: &str) -> bool {
    if needle.contains('(') {
        return source.text.contains(needle);
    }
    let needle: Vec<&str> = needle.split("::").collect();
    source
        .paths
        .iter()
        .any(|path| path_uses(path, &needle, &source.text))
}

/// The first file that uses `kit`, if any does.
fn first_use<'a>(kit: &Kit, sources: &'a [Source]) -> Option<&'a Path> {
    sources
        .iter()
        .find(|source| kit.paths.iter().any(|needle| source_uses(source, needle)))
        .map(|source| source.path.as_path())
}

fn kit_items() -> impl Iterator<Item = (&'static Item, &'static Kit)> {
    items().filter_map(|item| item.kit.as_ref().map(|kit| (item, kit)))
}

#[cfg(test)]
mod tests;
