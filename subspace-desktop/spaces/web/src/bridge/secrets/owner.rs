//! Whose a page's own secrets are. They are stored as `<owner>:<key>`, where
//! the owner is the first 32 hex digits of a digest of the app's install
//! source, so an update from the same source keeps them and another app
//! never reads them:
//!
//! - a GitHub repository install: the repository and board folder;
//! - a GitHub release install: the repository and asset;
//! - a local-folder install: the canonical folder it was copied from;
//! - a website: its origin.
//!
//! A ZIP install, and an entry whose source was not recorded, are strict:
//! the owner is the app identity grants use (folder and manifest), so a
//! reinstall starts with none.

use serde_json::{Map, Value};
use subspace_framework::core::config::web_boards::WebBoardSource;

use crate::install::ManagedSource;

/// The hex digits of an owner: 128 bits, so no other source or manifest is
/// found with the same ones.
pub(crate) const OWNER_CHARS: usize = 32;

/// Who the running app is, for its page's own secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    /// What new saves are stored under.
    prefix: String,
    /// The folder-and-manifest owner earlier builds saved under, still read
    /// until the page saves that key again.
    identity: String,
    /// Whether secrets saved before they had owners are this app's: only
    /// when its source is recorded and names it.
    adopts_ownerless: bool,
}

impl Owner {
    /// The owner of the app with grant identity `identity`, shown from
    /// `source`, with `managed` its recorded install source if any.
    pub(crate) fn new(
        identity: &str,
        source: &WebBoardSource,
        managed: Option<&ManagedSource>,
    ) -> Self {
        let identity = short(identity).to_owned();
        let named = source_text(source, managed).map(|text| digest(&text));
        Self {
            adopts_ownerless: named.is_some(),
            prefix: named.unwrap_or_else(|| identity.clone()),
            identity,
        }
    }

    /// The key a new save of page key `key` is stored under.
    pub(crate) fn stored_key(&self, key: &str) -> String {
        format!("{}:{key}", self.prefix)
    }

    /// Every stored key this app may hold page key `key` under: a save
    /// replaces them all with [`Self::stored_key`], and a delete removes them.
    pub(crate) fn forms(&self, key: &str) -> Vec<String> {
        let mut forms = vec![self.stored_key(key)];
        if self.identity != self.prefix {
            forms.push(format!("{}:{key}", self.identity));
        }
        if self.adopts_ownerless {
            forms.push(key.to_owned());
        }
        forms
    }

    /// The page key of `stored` when it is this app's, with how old its form
    /// is: 0 for the owner's, 1 for the earlier folder-and-manifest one, 2
    /// for one saved before secrets had owners. The caller leaves out
    /// ownerless keys the manifest declares.
    pub(crate) fn claims<'k>(&self, stored: &'k str) -> Option<(&'k str, u8)> {
        let under = |owner: &str| stored.strip_prefix(owner)?.strip_prefix(':');
        if let Some(key) = under(&self.prefix) {
            return Some((key, 0));
        }
        if let Some(key) = under(&self.identity) {
            return Some((key, 1));
        }
        (self.adopts_ownerless && !stored.contains(':')).then_some((stored, 2))
    }
}

/// What names where an app was installed from, or `None` when nothing does:
/// a ZIP, no recorded source, or a record that does not match the source.
fn source_text(source: &WebBoardSource, managed: Option<&ManagedSource>) -> Option<String> {
    match (source, managed?) {
        (
            WebBoardSource::Folder(_),
            ManagedSource::Repository {
                repository,
                board_path,
                ..
            },
        ) => Some(format!(
            "github\0{}\0{board_path}",
            repository.to_ascii_lowercase()
        )),
        (
            WebBoardSource::Folder(_),
            ManagedSource::Release {
                repository,
                asset_name,
                ..
            },
        ) => Some(format!(
            "github-release\0{}\0{asset_name}",
            repository.to_ascii_lowercase()
        )),
        (WebBoardSource::Folder(_), ManagedSource::Folder { original, .. }) => {
            Some(format!("folder\0{}", original.display()))
        }
        (WebBoardSource::Url(url), ManagedSource::Website) => url::Url::parse(url)
            .ok()
            .map(|url| format!("website\0{}", url.origin().ascii_serialization())),
        _ => None,
    }
}

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(format!("subspace web app\0{text}")));
    digest[..OWNER_CHARS].to_owned()
}

fn short(identity: &str) -> &str {
    identity.get(..OWNER_CHARS).unwrap_or(identity)
}

/// Whether stored `key` is a secret some app's page saved under an owner,
/// rather than one the user set.
pub(crate) fn is_owned(key: &str) -> bool {
    key.split_once(':').is_some_and(|(owner, _)| {
        owner.len() == OWNER_CHARS && owner.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// The recorded install source in a raw `webBoards` entry.
pub(crate) fn recorded(entry: &Map<String, Value>) -> Option<ManagedSource> {
    serde_json::from_value(entry.get("managedSource")?.clone()).ok()
}

/// Before entry `entry` takes another install source, binds the page
/// secrets that have no source owner yet to the source they were saved
/// under, so they carry over only when the new source is the same:
///
/// - ones saved before secrets had owners, unless the new app declares the
///   key (`declared`): then it is one of its settings;
/// - ones saved under the previous app's folder-and-manifest identity,
///   `identity`, when the entry's recorded source names that app.
///
/// When no recorded source names the previous app, ownerless ones go to an
/// owner no app has: hidden, but still counted in Settings.
pub(crate) fn bind_unowned(
    entry: &mut Map<String, Value>,
    declared: &[String],
    identity: Option<&str>,
) {
    let Some(source) = previous_source(entry) else {
        return;
    };
    let named = source_text(&source, recorded(entry).as_ref());
    let identity = identity.filter(|_| named.is_some()).map(short);
    let owner = digest(&named.unwrap_or_else(|| format!("unrecorded\0{}", source_label(&source))));
    let Some(Value::Object(secrets)) = entry.get_mut("secrets") else {
        return;
    };
    let unowned: Vec<(String, String)> = secrets
        .keys()
        .filter_map(|stored| {
            let key = match identity {
                Some(identity) => stored
                    .strip_prefix(identity)
                    .and_then(|rest| rest.strip_prefix(':')),
                None => None,
            };
            let key = key.or_else(|| {
                (!stored.contains(':') && !declared.contains(stored)).then_some(stored.as_str())
            })?;
            Some((stored.clone(), format!("{owner}:{key}")))
        })
        .collect();
    // A value already under the owner is newer than an unowned one.
    for (stored, bound) in unowned {
        if let Some(value) = secrets.remove(&stored) {
            secrets.entry(bound).or_insert(value);
        }
    }
}

fn previous_source(entry: &Map<String, Value>) -> Option<WebBoardSource> {
    match (entry.get("url"), entry.get("path")) {
        (Some(Value::String(url)), _) => Some(WebBoardSource::Url(url.clone())),
        (_, Some(Value::String(path))) => Some(WebBoardSource::Folder(path.into())),
        _ => None,
    }
}

fn source_label(source: &WebBoardSource) -> String {
    match source {
        WebBoardSource::Url(url) => url.clone(),
        WebBoardSource::Folder(path) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests;
