//! Conservative dotenv data import. No shell execution or value expansion.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read as _;

pub const MAX_INPUT_BYTES: usize = 128 * 1024;
pub const MAX_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_VARIABLES: usize = 64;

/// Imported values have names-only Debug output and no serialization support.
#[derive(Default)]
pub struct EnvironmentValues(BTreeMap<String, String>);

impl EnvironmentValues {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Explicitly transfer raw values to the private-settings boundary.
    pub fn into_values(self) -> BTreeMap<String, String> {
        self.0
    }
}

impl fmt::Debug for EnvironmentValues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentValues")
            .field("names", &self.names().collect::<Vec<_>>())
            .finish()
    }
}

/// Parse assignments with diagnostics that never contain source lines or values.
pub fn parse(source: &str) -> std::io::Result<EnvironmentValues> {
    let source = checked_source(source).map_err(|reason| parse_error(1, reason))?;
    let mut values = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        let record = parse_line(line).map_err(|reason| parse_error(index + 1, reason))?;
        insert_record(&mut values, record).map_err(|reason| parse_error(index + 1, reason))?;
    }
    Ok(EnvironmentValues(values))
}

/// Bound reads before allocating an imported file's entire contents.
pub fn parse_reader(reader: impl std::io::Read) -> std::io::Result<EnvironmentValues> {
    let mut source = String::new();
    reader
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_string(&mut source)?;
    parse(&source)
}

fn parse_error(line: usize, reason: &'static str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Dotenv line {line}: {reason}"),
    )
}

fn checked_source(source: &str) -> Result<&str, &'static str> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("input exceeds the import limit");
    }
    if source.contains('\0') {
        return Err("NUL characters are not supported");
    }
    Ok(source.strip_prefix('\u{feff}').unwrap_or(source))
}

fn parse_line(line: &str) -> Result<Option<(String, String)>, &'static str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let line = line
        .strip_prefix("export ")
        .or_else(|| line.strip_prefix("export\t"))
        .unwrap_or(line)
        .trim_start();
    let (name, source) = line
        .split_once('=')
        .ok_or("expected NAME=value assignment")?;
    let name = checked_name(name.trim())?;
    let value = parse_value(source.trim())?;
    Ok(Some((name.to_owned(), value)))
}

fn checked_name(name: &str) -> Result<&str, &'static str> {
    valid_name(name)
        .then_some(name)
        .ok_or("invalid environment variable name")
}

fn parse_value(source: &str) -> Result<String, &'static str> {
    let value = match source.chars().next() {
        Some(quote @ ('\'' | '"')) => quoted_value(source, quote)?,
        _ => unquoted_value(source),
    };
    checked_value(value)
}

fn checked_value(value: String) -> Result<String, &'static str> {
    if value.len() > MAX_VALUE_BYTES {
        return Err("value exceeds the import limit");
    }
    if value.contains("$(") || value.contains("${") || value.contains('`') {
        return Err("shell and variable substitutions are not supported");
    }
    Ok(value)
}

fn unquoted_value(source: &str) -> String {
    let mut previous_whitespace = true;
    for (offset, character) in source.char_indices() {
        if character == '#' && previous_whitespace {
            return source[..offset].trim_end().to_owned();
        }
        previous_whitespace = character.is_whitespace();
    }
    source.trim_end().to_owned()
}

fn quoted_value(source: &str, quote: char) -> Result<String, &'static str> {
    let mut value = String::new();
    let mut escaped = false;
    for (offset, character) in source.char_indices().skip(1) {
        if escaped {
            value.push(escape(character)?);
            escaped = false;
        } else if character == '\\' && quote == '"' {
            escaped = true;
        } else if character == quote {
            return quoted_suffix(&source[offset + character.len_utf8()..]).map(|()| value);
        } else {
            value.push(character);
        }
    }
    Err("unterminated quoted value")
}

fn escape(character: char) -> Result<char, &'static str> {
    match character {
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        '\\' => Ok('\\'),
        '"' => Ok('"'),
        _ => Err("unsupported quoted escape"),
    }
}

fn quoted_suffix(suffix: &str) -> Result<(), &'static str> {
    let suffix = suffix.trim_start();
    if suffix.is_empty() || suffix.starts_with('#') {
        Ok(())
    } else {
        Err("unexpected text after quoted value")
    }
}

fn insert_record(
    values: &mut BTreeMap<String, String>,
    record: Option<(String, String)>,
) -> Result<(), &'static str> {
    let Some((name, value)) = record else {
        return Ok(());
    };
    // Imports remain unambiguous on Windows as well as case-sensitive hosts.
    if values
        .keys()
        .any(|existing| existing.eq_ignore_ascii_case(&name))
    {
        return Err("duplicate environment variable name");
    }
    if values.len() >= MAX_VARIABLES {
        return Err("too many environment variables");
    }
    values.insert(name, value);
    Ok(())
}

/// Names follow portable process-environment identifier rules.
pub fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && characters.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
#[path = "dotenv/tests.rs"]
mod tests;
