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
    #[serde(default)]
    pub(crate) removal: Removal,
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

    /// Which files the Attachments panel can save. NETT managed media is
    /// not a standalone attachment; Subspace stores its files together.
    pub fn can_save_attachment(&self) -> bool {
        self.is_standalone() || self.is_subspace_file()
    }

    /// Only an explicit server decision enables removal of a Subspace file.
    pub fn can_remove_attachment(&self) -> bool {
        self.is_standalone() || (self.is_subspace_file() && self.removal == Removal::Available)
    }

    /// Why a listed Subspace file can be saved but not removed.
    pub fn attachment_removal_notice(&self) -> Option<&'static str> {
        if !self.is_subspace_file() {
            return None;
        }
        match self.removal {
            Removal::Linked => Some("Linked from the description, a comment or a document."),
            Removal::Archived => Some("Unarchive the issue before removing its files."),
            Removal::Available => None,
            Removal::Unspecified => {
                Some("This server doesn’t say whether the file can be removed.")
            }
        }
    }

    pub(crate) fn is_subspace_file(&self) -> bool {
        self.binding == MediaBinding::Subspace && self.kind == "file"
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Removal {
    Available,
    Linked,
    Archived,
    #[default]
    #[serde(other)]
    Unspecified,
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
