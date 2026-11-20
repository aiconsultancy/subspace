//! Saved files: their safe facts and where they live.
use serde::Deserialize;

/// Safe saved-file facts from Subspace API comment media descriptors; no grants or paths.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueMedia {
    pub media_id: String,
    #[serde(default)]
    pub version: u32,
    pub kind: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub public_file_id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    /// How the file is read and removed. Set by the client, never read
    /// from a reply.
    #[serde(skip)]
    pub(crate) binding: MediaBinding,
}

impl IssueMedia {
    /// Whether this is a NETT standalone attachment, which can be saved and
    /// removed from the Attachments panel.
    pub fn is_standalone(&self) -> bool {
        matches!(self.binding, MediaBinding::Standalone { .. })
    }
}

/// Where a media file lives, which decides how its bytes are read and how
/// it is removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum MediaBinding {
    /// Subspace API's media routes.
    #[default]
    Subspace,
    /// NETT managed media in an issue description or comment, read through
    /// the media's own grant.
    Managed,
    /// A NETT standalone attachment on `issue`, read through a file grant
    /// and removed with its version.
    Standalone { issue: String },
}
