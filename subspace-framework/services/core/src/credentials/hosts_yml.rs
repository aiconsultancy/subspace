//! The github.com accounts in gh's `hosts.yml`, read as the work server's
//! `parseHostsYml` does: only mapping keys under `users:` and the `user:`
//! scalar. Token values are never kept.

use super::accounts::GitHubAccounts;

/// The longest GitHub login: `^[A-Za-z0-9][A-Za-z0-9_-]{0,38}$`.
const MAX_LOGIN_CHARS: usize = 39;

/// A GitHub login: `^[A-Za-z0-9][A-Za-z0-9_-]{0,38}$` (EMU `_shortcode`
/// allowed).
pub fn is_login(value: &str) -> bool {
    value.len() <= MAX_LOGIN_CHARS
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The parser's position in the file while it reads the lines.
#[derive(Default)]
struct Scan {
    accounts: GitHubAccounts,
    in_host: bool,
    host_indent: usize,
    users_indent: Option<usize>,
    user_key_indent: Option<usize>,
}

impl Scan {
    /// Starts or leaves the `github.com:` block.
    fn host(&mut self, indent: usize, key: &str, value: &str) {
        self.in_host = key == "github.com" && value.is_empty();
        self.host_indent = indent;
        self.users_indent = None;
        self.user_key_indent = None;
    }

    /// Keeps a login key at the first indent under `users:`.
    fn user(&mut self, indent: usize, key: &str) {
        let first = *self.user_key_indent.get_or_insert(indent);
        if indent == first {
            self.accounts.logins.push(key.to_owned());
        }
    }

    /// A key of the host block itself: `users:` or `user:`.
    fn host_key(&mut self, indent: usize, key: &str, value: &str) {
        self.users_indent = None;
        if key == "users" && value.is_empty() {
            self.users_indent = Some(indent);
        } else if key == "user" && !value.is_empty() {
            self.accounts.active = Some(unquote(value).to_owned());
        }
    }

    fn entry(&mut self, indent: usize, key: &str, value: &str) {
        if !self.in_host || indent <= self.host_indent {
            self.host(indent, key, value);
        } else if self.users_indent.is_some_and(|users| indent > users) {
            self.user(indent, key);
        } else {
            self.host_key(indent, key, value);
        }
    }

    fn finish(self) -> GitHubAccounts {
        let mut accounts = self.accounts;
        accounts.logins.retain(|login| is_login(login));
        accounts.active = accounts.active.filter(|login| is_login(login));
        accounts
    }
}

/// The indent, key and value of a YAML mapping line. Blank, comment and
/// non-mapping lines give `None`.
fn entry_of(raw: &str) -> Option<(usize, &str, &str)> {
    let line = strip_comment(raw);
    let trimmed = line.trim_start();
    if trimmed.trim_end().is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let indent = line.len() - trimmed.len();
    let (key, value) = key_value(trimmed)?;
    Some((indent, key, value))
}

/// Reads the github.com logins and active login from gh's `hosts.yml`.
pub fn parse_hosts_yml(text: &str) -> GitHubAccounts {
    let mut scan = Scan::default();
    for (indent, key, value) in text.lines().filter_map(entry_of) {
        scan.entry(indent, key, value);
    }
    scan.finish()
}

/// Removes a trailing comment: whitespace, `#`, then anything.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'#' && index > 0 && bytes[index - 1].is_ascii_whitespace() {
            return line[..index].trim_end();
        }
    }
    line
}

/// Splits `key: value` or `key:` as the pattern
/// `("[^"]*"|'[^']*'|[^:\s][^:]*?)\s*:(?:\s+(.*))?` does. `line` has no
/// leading whitespace.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = match line.chars().next()? {
        quote @ ('"' | '\'') => {
            let end = line[1..].find(quote)? + 2;
            (&line[1..end - 1], line[end..].trim_start())
        }
        ':' => return None,
        _ => {
            let colon = line.find(':')?;
            (line[..colon].trim_end(), &line[colon..])
        }
    };
    let after = rest.strip_prefix(':')?;
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some((key, after.trim()))
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}
