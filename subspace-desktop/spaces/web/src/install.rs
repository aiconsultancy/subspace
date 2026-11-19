//! Native built-file imports. Call off the UI thread. Prepared folders are
//! unique private snapshots, never overwritten. Save the source and provenance
//! atomically, then call `retain`; dropping an unsaved result removes only its
//! own temporary directory. The caller owns board IDs and stale-result checks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use subspace_framework::client_github::BoundRead;
pub use subspace_framework::client_github::RepositoryRef;
use subspace_framework::core::config::web_boards::{WebBoardSource, web_board_url_allowed};

mod archive;
mod files;
mod github;
mod paths;
mod stage;
#[cfg(test)]
mod tests;
mod update;
mod zip_header;
pub use update::{UpdateOffer, check_update};

/// Explicit native user choices, never constructed from an imported manifest.
#[derive(Clone)]
pub enum ImportRequest {
    Website {
        url: String,
    },
    Folder {
        path: PathBuf,
    },
    Zip {
        path: PathBuf,
    },
    Repository {
        repository: String,
        reference: RepositoryRef,
        board_path: String,
    },
    /// An offered update downloads exactly this commit while retaining the
    /// original branch tracking choice. Not a browsing-URL interpretation.
    RepositorySnapshot {
        repository: String,
        reference: RepositoryRef,
        board_path: String,
        commit: String,
    },
    Release {
        repository: String,
        release_id: u64,
        asset_name: String,
        include_prereleases: bool,
    },
}

/// Store this as the entry's `managedSource`. No credential, principal or signed
/// download URL is persisted. Unknown legacy entries need no provenance.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ManagedSource {
    Website,
    Folder {
        original: PathBuf,
        content: String,
    },
    Zip {
        original: PathBuf,
        content: String,
    },
    Repository {
        repository: String,
        reference: RepositoryRef,
        board_path: String,
        commit: String,
        content: String,
    },
    Release {
        repository: String,
        release_id: u64,
        tag: String,
        asset_name: String,
        include_prereleases: bool,
        content: String,
    },
}

impl ManagedSource {
    pub fn label(&self) -> String {
        match self {
            Self::Website => "Website".into(),
            Self::Folder { .. } => "Imported folder".into(),
            Self::Zip { .. } => "Imported ZIP".into(),
            Self::Repository { repository, .. } => format!("GitHub repository {repository}"),
            Self::Release {
                repository,
                asset_name,
                ..
            } => format!("GitHub release {repository}, {asset_name}"),
        }
    }
    pub fn revision(&self) -> Option<&str> {
        match self {
            Self::Repository { commit, .. } => Some(commit),
            Self::Release { tag, .. } => Some(tag),
            _ => None,
        }
    }
    pub fn repository(&self) -> Option<&str> {
        match self {
            Self::Repository { repository, .. } | Self::Release { repository, .. } => {
                Some(repository)
            }
            _ => None,
        }
    }
}

pub struct PreparedImport {
    snapshot: Snapshot,
}

struct Snapshot {
    source: WebBoardSource,
    metadata: ManagedSource,
    summary: Option<Summary>,
    owned: Option<tempfile::TempDir>,
}

struct Summary {
    name: String,
    version: String,
    permissions: Vec<String>,
    warnings: Vec<String>,
}

impl PreparedImport {
    pub fn source(&self) -> WebBoardSource {
        self.snapshot.source.clone()
    }
    pub fn metadata(&self) -> &ManagedSource {
        &self.snapshot.metadata
    }
    pub fn name(&self) -> Option<&str> {
        self.snapshot
            .summary
            .as_ref()
            .map(|summary| summary.name.as_str())
    }
    pub fn version(&self) -> Option<&str> {
        self.snapshot
            .summary
            .as_ref()
            .map(|summary| summary.version.as_str())
    }
    /// Permission names only. This import does not grant them. Existing grants
    /// are valid only under the existing canonical-path and manifest binding.
    pub fn permissions(&self) -> &[String] {
        self.snapshot
            .summary
            .as_ref()
            .map_or(&[], |summary| &summary.permissions)
    }
    pub fn warnings(&self) -> &[String] {
        self.snapshot
            .summary
            .as_ref()
            .map_or(&[], |summary| &summary.warnings)
    }
    /// Call only after persistence succeeds. Never removes old installations.
    pub fn retain(mut self) {
        if let Some(owned) = self.snapshot.owned.take() {
            let _ = owned.keep();
        }
    }
}

/// Prepare built assets with no Settings or UI dependency. GitHub choices
/// require the selected user's immutable binding, with no fallback account.
pub fn prepare(
    request: ImportRequest,
    store: &Path,
    github: Option<&BoundRead>,
) -> Result<PreparedImport, String> {
    match request {
        ImportRequest::Website { url } => website(url),
        request => stage::prepare(request, store, github),
    }
}

fn website(text: String) -> Result<PreparedImport, String> {
    let url = url::Url::parse(text.trim())
        .map_err(|_| "Enter an HTTPS website URL or loopback HTTP URL.")?;
    if !web_board_url_allowed(&url) || !url.username().is_empty() || url.password().is_some() {
        return Err("Use HTTPS or loopback HTTP, without embedded credentials.".into());
    }
    Ok(PreparedImport {
        snapshot: Snapshot {
            source: WebBoardSource::Url(url.into()),
            metadata: ManagedSource::Website,
            summary: None,
            owned: None,
        },
    })
}
