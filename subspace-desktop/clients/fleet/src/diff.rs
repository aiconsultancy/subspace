//! Paged, read-only crew worktree diffs. Diff bodies deliberately do not
//! implement Debug and are not used by the fleet feed or its summaries.

use serde::Deserialize;
use url::Url;

use crate::{Error, FleetAddress, FleetClient};

// One list page can carry 1,000 × 64 KiB of patch text before JSON escaping;
// allow six bytes per input byte plus metadata. File pages hold up to 1 MiB.
const MAX_DIFF_BYTES: u64 = 400 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 7 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    #[default]
    Branch,
    Working,
    Committed,
}

impl DiffMode {
    fn query(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Working => "working",
            Self::Committed => "committed",
        }
    }
}

#[derive(Clone, Copy)]
pub struct DiffTarget<'a> {
    pub ship: &'a str,
    pub member: &'a str,
    pub project: Option<&'a str>,
    pub mode: DiffMode,
}

#[derive(Clone, Copy)]
pub struct DiffFilePage<'a> {
    pub path: &'a str,
    pub page: Option<&'a str>,
    pub at: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V1Diff {
    pub ship: String,
    pub member: String,
    pub project: Option<String>,
    pub mode: DiffMode,
    pub branch: Option<String>,
    pub base: Option<DiffBase>,
    pub head: Option<String>,
    pub fingerprint: String,
    pub computed_at: u64,
    pub total: u32,
    pub additions: u32,
    pub deletions: u32,
    pub files: Vec<V1DiffFile>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct DiffBase {
    pub r#ref: String,
    pub commit: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V1DiffFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: DiffStatus,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    #[serde(default)]
    pub untracked: bool,
    #[serde(default)]
    pub binary: bool,
    /// The binary file's size in bytes; the old side for a removal.
    pub size: Option<u64>,
    pub patch: Option<String>,
    pub patch_next_page: Option<String>,
    pub kind: Option<DiffKind>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
    Renamed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    Submodule,
    Symlink,
}

#[derive(Deserialize)]
pub struct V1DiffFilePage {
    pub fingerprint: String,
    pub file: V1DiffFile,
}

// qual:allow(coupling, oi) reason: "These read-only endpoints compose the query encoder with the existing credential-bound transport."
impl FleetClient {
    /// Read one page of changed files. `None` means the route is unavailable.
    pub fn diff(
        &self,
        target: DiffTarget<'_>,
        cursor: Option<&str>,
    ) -> Result<Option<V1Diff>, Error> {
        let mut url = target.url(self.address(), false)?;
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        match self.get_json(&url, MAX_DIFF_BYTES) {
            Ok(page) => Ok(Some(page)),
            Err(Error::NotFound) if cursor.is_none() => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Read one page of a file. Unlike a list-route 404, a file-route 404
    /// can mean that the requested path does not exist.
    pub fn diff_file(
        &self,
        target: DiffTarget<'_>,
        request: DiffFilePage<'_>,
    ) -> Result<V1DiffFilePage, Error> {
        let mut url = target.url(self.address(), true)?;
        url.query_pairs_mut()
            .append_pair("path", request.path)
            .append_pair("at", request.at);
        if let Some(page) = request.page {
            url.query_pairs_mut().append_pair("page", page);
        }
        self.get_json(&url, MAX_FILE_BYTES)
            .map_err(|error| match error {
                Error::UnexpectedStatus(409) => Error::StaleDiff,
                other => other,
            })
    }
}

impl DiffTarget<'_> {
    fn url(self, address: &FleetAddress, file: bool) -> Result<Url, Error> {
        let mut segments = vec!["ships", self.ship, "crew", self.member, "diff"];
        if file {
            segments.push("file");
        }
        let mut url = address.url(&segments)?;
        url.query_pairs_mut().append_pair("mode", self.mode.query());
        if let Some(project) = self.project {
            url.query_pairs_mut().append_pair("project", project);
        }
        Ok(url)
    }
}
