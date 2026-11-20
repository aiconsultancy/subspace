//! Bounded multipart intake. A POST is sent once; recovery only reads metadata.
mod transport;
mod validation;
pub use crate::media::MAX_IMAGE_BYTES;
use sha2::{Digest as _, Sha256};
pub(crate) use validation::document;
pub use validation::{file_mime, max_bytes};
pub const MAX_IMAGE_EDGE: u32 = 4096;
pub const MAX_IMAGE_PIXELS: u64 = 16_777_216;
pub const MAX_FILE_BYTES: usize = 10_000_000;
pub const MAX_DRAFT_IMAGES: usize = 8;
pub const MAX_DRAFT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct ImageDeclaration {
    pub(crate) file_name: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: usize,
    pub(crate) sha256: String,
}
impl std::fmt::Debug for ImageDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageDeclaration([redacted])")
    }
}
impl ImageDeclaration {
    pub fn new(name: &str, mime: &str, bytes: &[u8]) -> Result<Self, UploadError> {
        let name = validation::normalize_name(name)?;
        validation::validate(&name, mime, bytes)?;
        Ok(Self {
            file_name: name,
            content_type: mime.into(),
            size_bytes: bytes.len(),
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
    }
    pub fn is_image(&self) -> bool {
        crate::media::supported_image(&self.content_type)
    }
    pub(crate) fn matches(&self, media: &crate::model::IssueMedia) -> bool {
        validation::normalize_name(&media.file_name).as_ref() == Ok(&self.file_name)
            && self.size_bytes as i64 == media.size_bytes
            && self.sha256.eq_ignore_ascii_case(&media.sha256)
    }
    /// Whether a record giving these fields, any of which may be missing,
    /// shows another file than this one: a name, size or SHA-256 it gives
    /// that differs. A field it leaves out shows nothing either way.
    pub(crate) fn contradicted_by(
        &self,
        name: Option<&str>,
        size: Option<i64>,
        sha256: Option<&str>,
    ) -> bool {
        name.is_some_and(|name| {
            validation::normalize_name(name).is_ok_and(|name| name != self.file_name)
        }) || size.is_some_and(|size| size != self.size_bytes as i64)
            || sha256.is_some_and(|sha256| !self.sha256.eq_ignore_ascii_case(sha256))
    }
    pub(crate) fn unchanged(&self, bytes: &[u8]) -> bool {
        self.size_bytes == bytes.len()
            && self.sha256
                == Sha256::digest(bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
    }
}

/// One standalone attachment upload: its outcome, the transfer it reserved
/// and whether its bytes may have been stored. Once they may have been, a
/// failure is resolved by completing that transfer, not by sending again.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AttachmentAttempt {
    pub result: Result<crate::model::IssueMedia, UploadError>,
    pub transfer: Option<String>,
    pub bytes_sent: bool,
}

impl Default for AttachmentAttempt {
    fn default() -> Self {
        Self {
            result: Err(UploadError::Network),
            transfer: None,
            bytes_sent: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    #[error("Choose a supported image (up to 10 MiB and 4,096 px per side) or file (up to 10 MB).")]
    InvalidImage,
    #[error("Keep at most eight attachments and 32 MiB in this draft, and 64 MiB across drafts.")]
    Budget,
    #[error("The upload may have completed. Choose Check or Remove; it will not be sent again.")]
    Checking,
    #[error("The upload may have completed. Choose Check or Remove; it will not be sent again.")]
    Unclear { retry_after: u64 },
    #[error("The service could not be reached. Your attachment is still here.")]
    Network,
    #[error("Sign in under Settings › Account, then check the attachment.")]
    SignInNeeded,
    #[error("The service refused this attachment ({status}).")]
    Refused { status: u16, retry_after: u64 },
    #[error("This attachment was removed.")]
    Removed,
    #[error("Couldn't remove this upload. Choose Remove to try again.")]
    Cleanup,
}
impl UploadError {
    pub fn message(self) -> String {
        self.to_string()
    }
    pub fn retryable(self) -> bool {
        !matches!(
            self,
            Self::InvalidImage | Self::Budget | Self::Removed | Self::Cleanup
        )
    }
}
