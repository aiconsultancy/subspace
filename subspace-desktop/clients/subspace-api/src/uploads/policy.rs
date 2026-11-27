//! What the selected service requires of issue attachment reads and writes.
use super::{ImageDeclaration, MAX_FILE_BYTES, UploadError, max_bytes};
use crate::{Backend, Client, Error};

/// The standalone attachment rules for a client's selected service.
#[derive(Clone, Copy)]
pub enum AttachmentProtocol {
    NettStandalone,
    SubspaceFiles,
}

impl AttachmentProtocol {
    /// Maximum size of a standalone upload, in bytes.
    pub fn max_bytes(self, mime: &str) -> usize {
        match self {
            Self::NettStandalone => max_bytes(mime),
            Self::SubspaceFiles => MAX_FILE_BYTES,
        }
    }

    pub(crate) fn declare(
        self,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<ImageDeclaration, UploadError> {
        ImageDeclaration::with_limit(name, mime, bytes, self.max_bytes(mime))
            .map_err(|_| self.invalid_file())
    }

    /// Error shown when a standalone file cannot be read or validated.
    pub fn invalid_file(self) -> UploadError {
        match self {
            Self::NettStandalone => UploadError::InvalidImage,
            Self::SubspaceFiles => UploadError::UnsupportedFile,
        }
    }

    pub(crate) fn hidden_endpoint(self, error: Error) -> bool {
        matches!(self, Self::SubspaceFiles)
            && matches!(error, Error::NotFound | Error::UnexpectedStatus(405 | 501))
    }

    pub(crate) fn accepts_files(self, listed: bool) -> bool {
        matches!(self, Self::NettStandalone) || listed
    }

    pub(crate) fn checkable_uploads(self) -> bool {
        matches!(self, Self::NettStandalone)
    }

    pub(crate) fn conflict_notice(self) -> &'static str {
        match self {
            Self::NettStandalone => "This attachment changed elsewhere. The list was read again.",
            Self::SubspaceFiles => "Couldn't remove the attachment. The list was read again.",
        }
    }

    pub(crate) fn wait_notice(self) -> &'static str {
        match self {
            Self::NettStandalone => "NETT asked to wait before this upload is checked again.",
            Self::SubspaceFiles => "Subspace API asked to wait before trying this upload again.",
        }
    }
}

impl Client {
    pub(crate) fn attachment_protocol(&self) -> AttachmentProtocol {
        match self.backend {
            Backend::Nett => AttachmentProtocol::NettStandalone,
            Backend::Subspace => AttachmentProtocol::SubspaceFiles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentProtocol, MAX_FILE_BYTES, UploadError};
    use crate::uploads::MAX_IMAGE_BYTES;

    #[test]
    fn subspace_rasters_use_the_file_limit_but_nett_still_uses_the_image_limit() {
        let subspace = AttachmentProtocol::SubspaceFiles;
        let nett = AttachmentProtocol::NettStandalone;
        assert_eq!(subspace.max_bytes("image/png"), MAX_FILE_BYTES);
        assert_eq!(nett.max_bytes("image/png"), MAX_IMAGE_BYTES);
        assert_eq!(nett.max_bytes("text/plain"), MAX_FILE_BYTES);
        assert_eq!(
            subspace.declare("invalid.png", "image/png", b"not an image"),
            Err(UploadError::UnsupportedFile)
        );
    }
}
