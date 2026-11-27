//! Media metadata and bounded bytes use the same private API transport as JSON.
use crate::{
    Client, Error, PageRequest,
    model::{IssueMedia, Page},
    transport,
    uploads::AttachmentProtocol,
};
use serde::Deserialize;

const MAX_MEDIA_PAGE: u32 = 100;
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// The largest text attachment the issue viewer reads and shows.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadata {
    pub id: String,
    pub issue_id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub url: String,
    pub sha256: String,
    pub created_at: String,
    pub uploaded_by: crate::model::Person,
}

#[derive(Debug, Clone)]
pub struct IssueImage {
    pub media: IssueMedia,
    pub alt_text: String,
    pub comment_id: Option<String>,
}

impl ImageMetadata {
    pub(crate) fn descriptor(self) -> IssueMedia {
        IssueMedia {
            media_id: self.id,
            version: 1,
            kind: "image".into(),
            file_name: self.file_name,
            content_type: self.content_type,
            size_bytes: self.size_bytes,
            url: self.url,
            sha256: self.sha256,
            public_file_id: String::new(),
            expires_at: None,
            removal: Default::default(),
            binding: crate::model::MediaBinding::Subspace,
        }
    }
}

impl std::fmt::Debug for ImageMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageMetadata")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

pub fn supported_image(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// Whether an attachment is text the issue viewer can show.
pub fn readable_text(mime: &str) -> bool {
    matches!(mime, "text/plain" | "text/markdown")
}

// qual:allow(coupling, oi) reason: "Media reads compose validated paths, paced private transport and bounded response checks."
impl Client {
    pub fn issue_images(
        &self,
        issue: &str,
        page: &PageRequest,
    ) -> Result<Page<ImageMetadata>, Error> {
        if self.backend == crate::Backend::Nett {
            return self.nett_images(issue, page);
        }
        self.get(
            &["issues", issue, "images"],
            &page.parameters(MAX_MEDIA_PAGE)?,
        )
    }

    pub fn attachments(&self, issue: &str, page: &PageRequest) -> Result<Page<IssueMedia>, Error> {
        if let AttachmentProtocol::NettStandalone = self.attachment_protocol() {
            return self.nett_attachments(issue, page);
        }
        self.get(
            &["issues", issue, "attachments"],
            &page.parameters(MAX_MEDIA_PAGE)?,
        )
    }

    pub fn fetch_image(&self, media: &IssueMedia, limit: usize) -> Result<Vec<u8>, Error> {
        if !supported_image(&media.content_type) {
            return Err(Error::InvalidRequest);
        }
        if self.backend == crate::Backend::Nett {
            return self.nett_media_bytes(media, limit.min(MAX_IMAGE_BYTES));
        }
        self.fetch_media("media", media, limit.min(MAX_IMAGE_BYTES))
    }

    /// A text attachment's bytes from the attachment read, under the same
    /// rules as image bytes.
    pub fn fetch_text(&self, media: &IssueMedia, limit: usize) -> Result<Vec<u8>, Error> {
        if !readable_text(&media.content_type) {
            return Err(Error::InvalidRequest);
        }
        if self.backend == crate::Backend::Nett {
            return self.nett_media_bytes(media, limit.min(MAX_TEXT_BYTES));
        }
        self.fetch_media("attachments", media, limit.min(MAX_TEXT_BYTES))
    }

    /// A listed attachment's bytes, for saving, with exact type, length
    /// and `nosniff`. NETT standalone files use a private file grant.
    pub fn fetch_file(&self, media: &IssueMedia, limit: usize) -> Result<Vec<u8>, Error> {
        let protocol = self.attachment_protocol();
        match protocol {
            AttachmentProtocol::NettStandalone if media.is_standalone() => {
                self.nett_media_bytes(media, limit.min(crate::chat_uploads::MAX_BYTES))
            }
            AttachmentProtocol::SubspaceFiles if media.is_subspace_file() => {
                let max = protocol.max_bytes(&media.content_type);
                self.fetch_media("attachments", media, limit.min(max))
            }
            _ => Err(Error::InvalidRequest),
        }
    }

    /// Exact stored type, `nosniff`, a bounded body and the declared length.
    fn fetch_media(&self, route: &str, media: &IssueMedia, limit: usize) -> Result<Vec<u8>, Error> {
        if media.size_bytes <= 0 {
            return Err(Error::InvalidRequest);
        }
        let limit = limit.min(media.size_bytes as usize);
        let url = transport::endpoint(&self.base, &["issues", route, &media.media_id], &[])?;
        let bytes = transport::get_bytes(
            &self.agent,
            &url,
            self.access_jwt.as_ref(),
            (&media.content_type, limit),
        )?;
        if bytes.len() as i64 != media.size_bytes {
            return Err(Error::InvalidResponse);
        }
        Ok(bytes)
    }
}
