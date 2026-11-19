//! Low-level Ghostty configuration syntax: line splitting, quoting, Zig
//! string escapes and `config-file` include resolution.
//!
//! This module has no knowledge of which keys are supported; [`load_all`]
//! returns every `key = value` pair it finds, in file-then-include order,
//! for [`crate::keys`] to apply.

use crate::warning::Warning;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// One `key = value` line from a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub key: String,
    /// `None` means the line had an empty value, which resets the key to its
    /// default.
    pub value: Option<String>,
    pub file: PathBuf,
    pub line: usize,
}

/// Reads every root file and its `config-file` includes, in REQ-29 order.
/// Missing files are not an error. A cycle produces one warning and stops
/// that branch.
pub(crate) fn load_all(roots: &[PathBuf]) -> (Vec<Entry>, Vec<Warning>) {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for root in roots {
        load_root(root, &mut entries, &mut warnings);
    }
    (entries, warnings)
}

/// Integration: walks one root file and its include tree, breadth-first so
/// each file's own lines apply before its includes (REQ-29).
fn load_root(root: &Path, entries: &mut Vec<Entry>, warnings: &mut Vec<Warning>) {
    let mut loaded: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<(PathBuf, bool)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), true));
    while let Some((path, missing_is_quiet)) = queue.pop_front() {
        if loaded.contains(&path) {
            warnings.push(Warning::new(&path, 0, "config-file: cycle detected"));
            continue;
        }
        loaded.insert(path.clone());
        let Some(lines) = read_lines(&path) else {
            let _ = missing_is_quiet;
            continue;
        };
        apply_file_lines(&path, &lines, entries, &mut queue);
    }
}

/// Operation: turns one file's non-comment lines into applied entries and
/// queued `config-file` includes.
fn apply_file_lines(
    path: &Path,
    lines: &[(usize, String)],
    entries: &mut Vec<Entry>,
    queue: &mut VecDeque<(PathBuf, bool)>,
) {
    for (line_no, raw) in lines {
        let Some((key, value)) = parse_key_value(raw) else {
            continue;
        };
        if key == "config-file" {
            if let Some(include) = resolve_include(path, value.as_deref().unwrap_or("")) {
                queue.push_back(include);
            }
            continue;
        }
        entries.push(Entry {
            key,
            value,
            file: path.to_path_buf(),
            line: *line_no,
        });
    }
}

/// Operation: reads a file into trimmed, non-blank, non-comment lines with
/// their 1-indexed line numbers. Returns `None` when the file cannot be
/// read, which is not an error under REQ-29.
pub(crate) fn read_lines(path: &Path) -> Option<Vec<(usize, String)>> {
    let text = fs::read_to_string(path).ok()?;
    let mut result = Vec::new();
    for (idx, raw_line) in text.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        result.push((idx + 1, trimmed.to_string()));
    }
    Some(result)
}

/// Operation: splits a trimmed line into a key and an optional value,
/// stripping quotes and decoding Zig string escapes in a quoted value. A
/// line with no `=` is ignored.
pub(crate) fn parse_key_value(line: &str) -> Option<(String, Option<String>)> {
    let idx = line.find('=')?;
    let key = line[..idx].trim().to_string();
    let raw_value = line[idx + 1..].trim();
    let (value, _was_quoted) = unquote(raw_value);
    if value.is_empty() {
        Some((key, None))
    } else {
        Some((key, Some(value)))
    }
}

/// Operation: strips surrounding double quotes and decodes Zig string
/// escapes inside them. An unquoted value is returned unchanged.
fn unquote(value: &str) -> (String, bool) {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let inner = &value[1..value.len() - 1];
        (decode_zig_escapes(inner), true)
    } else {
        (value.to_string(), false)
    }
}

/// Operation: decodes the Zig string literal escapes Ghostty accepts inside
/// a quoted configuration value: `\n`, `\t`, `\r`, `\\`, `\"`, `\'`, `\0` and
/// `\xHH`. An unrecognised escape keeps its backslash and following
/// character literally.
fn decode_zig_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('0') => out.push('\0'),
            Some('x') => push_hex_byte(&mut out, &mut chars),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Operation: consumes two hex digits after `\x` and pushes the decoded
/// byte, or the literal text when the digits are missing or invalid.
fn push_hex_byte(out: &mut String, chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let digits: String = chars.by_ref().take(2).collect();
    match u8::from_str_radix(&digits, 16) {
        Ok(byte) => out.push(byte as char),
        Err(_) => {
            out.push_str("\\x");
            out.push_str(&digits);
        }
    }
}

/// Operation: resolves a `config-file` value relative to the file that
/// contains it. A leading `?` (on an unquoted value) marks the include
/// optional; a quoted value takes its path literally, including a leading
/// `?`.
fn resolve_include(including_file: &Path, raw_value: &str) -> Option<(PathBuf, bool)> {
    if raw_value.is_empty() {
        return None;
    }
    let (optional, path_str) =
        if raw_value.len() >= 2 && raw_value.starts_with('"') && raw_value.ends_with('"') {
            (false, &raw_value[1..raw_value.len() - 1])
        } else if let Some(stripped) = raw_value.strip_prefix('?') {
            (true, stripped)
        } else {
            (false, raw_value)
        };
    if path_str.is_empty() {
        return None;
    }
    let path = Path::new(path_str);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        including_file.parent()?.join(path)
    };
    Some((resolved, optional))
}

#[cfg(test)]
mod tests {
    use super::{decode_zig_escapes, load_all, parse_key_value, resolve_include};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    #[test]
    fn parse_key_value_splits_trimmed_key_and_value() {
        let (key, value) = parse_key_value("font-size = 13").unwrap();
        assert_eq!(key, "font-size");
        assert_eq!(value.as_deref(), Some("13"));
    }

    #[test]
    fn parse_key_value_strips_quotes() {
        let (key, value) = parse_key_value(r#"term = "xterm-ghostty""#).unwrap();
        assert_eq!(key, "term");
        assert_eq!(value.as_deref(), Some("xterm-ghostty"));
    }

    #[test]
    fn parse_key_value_empty_value_resets_to_default() {
        let (key, value) = parse_key_value("background = ").unwrap();
        assert_eq!(key, "background");
        assert_eq!(value, None);
    }

    #[test]
    fn parse_key_value_ignores_a_line_with_no_equals() {
        assert_eq!(parse_key_value("not-a-directive"), None);
    }

    #[test]
    fn decode_zig_escapes_handles_known_escapes() {
        assert_eq!(decode_zig_escapes(r"a\nb\tc\\d"), "a\nb\tc\\d");
        assert_eq!(decode_zig_escapes(r"\x0a"), "\n");
    }

    #[test]
    fn resolve_include_is_relative_to_the_including_file() {
        let (path, optional) = resolve_include(
            Path::new("/home/fixture/.config/ghostty/config"),
            "extra.conf",
        )
        .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/fixture/.config/ghostty/extra.conf")
        );
        assert!(!optional);
    }

    #[test]
    fn resolve_include_strips_optional_marker() {
        let (path, optional) = resolve_include(
            Path::new("/home/fixture/.config/ghostty/config"),
            "?extra.conf",
        )
        .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/fixture/.config/ghostty/extra.conf")
        );
        assert!(optional);
    }

    #[test]
    fn resolve_include_keeps_a_quoted_literal_question_mark() {
        let (path, optional) = resolve_include(
            Path::new("/home/fixture/.config/ghostty/config"),
            "\"?weird.conf\"",
        )
        .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/fixture/.config/ghostty/?weird.conf")
        );
        assert!(!optional);
    }

    #[test]
    fn load_all_follows_an_include_after_the_including_file() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("config");
        let included_path = dir.path().join("included");
        std::fs::write(&included_path, "font-size = 20\n").unwrap();
        let mut main_file = std::fs::File::create(&main_path).unwrap();
        writeln!(main_file, "config-file = included").unwrap();
        writeln!(main_file, "font-size = 10").unwrap();
        drop(main_file);

        let (entries, warnings) = load_all(&[main_path]);
        assert!(warnings.is_empty());
        let sizes: Vec<_> = entries
            .iter()
            .filter(|entry| entry.key == "font-size")
            .map(|entry| entry.value.clone())
            .collect();
        assert_eq!(sizes, vec![Some("10".to_string()), Some("20".to_string())]);
    }

    #[test]
    fn load_all_stops_an_include_cycle_with_one_warning() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a");
        let b_path = dir.path().join("b");
        std::fs::write(&a_path, "config-file = b\n").unwrap();
        std::fs::write(&b_path, "config-file = a\n").unwrap();

        let (_entries, warnings) = load_all(&[a_path]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].reason.contains("cycle"));
    }

    #[test]
    fn load_all_treats_a_missing_file_as_not_an_error() {
        let (entries, warnings) = load_all(&[PathBuf::from("/no/such/file")]);
        assert!(entries.is_empty());
        assert!(warnings.is_empty());
    }
}
