//! Paged, read-only crew worktree diffs. Diff bodies deliberately do not
//! implement Debug and are not used by the fleet feed or its summaries.

use serde::Deserialize;
use url::Url;

use crate::{Error, FleetAddress};

mod binary;
mod read;

pub use binary::{DiffContent, DiffSide};
pub use read::DiffRead;

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
    pub side: Option<DiffSide>,
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
    #[serde(default)]
    pub unstable: bool,
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
    #[serde(default)]
    pub unstable: bool,
    pub file: V1DiffFile,
    pub content: Option<DiffContent>,
}

impl DiffTarget<'_> {
    pub(super) fn list_url(
        self,
        address: &FleetAddress,
        cursor: Option<&str>,
    ) -> Result<Url, Error> {
        let mut url = self.url(address, false)?;
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        Ok(url)
    }

    pub(super) fn file_url(
        self,
        address: &FleetAddress,
        request: DiffFilePage<'_>,
    ) -> Result<Url, Error> {
        let mut url = self.url(address, true)?;
        url.query_pairs_mut()
            .append_pair("path", request.path)
            .append_pair("at", request.at);
        if let Some(page) = request.page {
            url.query_pairs_mut().append_pair("page", page);
        }
        if let Some(side) = request.side {
            url.query_pairs_mut().append_pair("side", side.query());
        }
        Ok(url)
    }

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
