//! Temporary chat files: private fixed-origin transport and stable idempotent upload IDs.
mod encoding;
mod lines;
mod transport;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
pub use lines::{lines, reference_spans, reference_text, without_references};
use serde::Deserialize;
use std::sync::Arc;

const PREFIX_LEN: usize = 4;
const PAYLOAD_LEN: usize = 22;
const UUID_BYTES: usize = 16;
const PREVIEW_BYTES: usize = 120;
const EMPTY_ZIP_HEADER_BYTES: usize = 22;
const KEY_LEN: usize = PREFIX_LEN + PAYLOAD_LEN;
pub const MAX_BYTES: usize = 25_000_000;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Upload {
    pub id: String,
    pub reference: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub uploaded_by_user_id: String,
    pub created_at: String,
    pub expires_at: String,
    /// Subspace API's content link. NETT gives none: its files are read
    /// through grants.
    #[serde(default)]
    pub url: String,
    /// NETT's version, needed to delete the upload.
    #[serde(default)]
    pub version: u32,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Card {
    pub reference: String,
    pub kind: String,
    pub status: String,
    pub upload: Option<Upload>,
}
#[derive(Deserialize)]
struct Cards {
    items: Vec<Card>,
}
#[derive(Clone)]
pub struct UploadDraft {
    id: String,
    reference: String,
    name: String,
    mime: String,
    bytes: Arc<[u8]>,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    #[error("Sign in under Settings › Account, then try again.")]
    Access,
    #[error("This upload has expired after 30 days.")]
    Expired,
    #[error("This upload is missing or has been deleted.")]
    Missing,
    #[error("You do not have permission to access this upload.")]
    Forbidden,
    #[error("Choose text, an image, ZIP, PDF, DOCX or XLSX, up to 25 MB.")]
    Invalid,
    #[error(
        "Use a valid PNG, JPEG, GIF or WebP, at most 4096 pixels on either edge and 16 megapixels in total."
    )]
    InvalidImage,
    #[error("Use a file name of 1 to 200 characters.")]
    InvalidName,
    #[error("This upload ID is already in use. Check the upload before trying again.")]
    Conflict,
    #[error("The upload may have completed. Check it before trying again.")]
    Unconfirmed,
    #[error("Couldn’t reach Subspace. Keep the draft and try again.")]
    Unavailable,
}
impl UploadDraft {
    pub fn new(name: String, mime: String, bytes: Vec<u8>) -> Result<Self, UploadError> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES || !supported(&mime) {
            return Err(UploadError::Invalid);
        }
        let name = clean_name(&name)?;
        if mime.starts_with("application/") {
            if mime_for_extension(name.rsplit('.').next().unwrap_or_default())
                != Some(mime.as_str())
            {
                return Err(UploadError::Invalid);
            }
            let valid = if mime == "application/zip" {
                bytes.starts_with(b"PK\x03\x04")
                    || (bytes.len() >= EMPTY_ZIP_HEADER_BYTES && bytes.starts_with(b"PK\x05\x06"))
            } else {
                crate::uploads::document(&bytes, &mime).is_ok()
            };
            if !valid {
                return Err(UploadError::Invalid);
            }
        }
        let id = uuid::Uuid::new_v4();
        let reference = format!("UPL-{}", URL_SAFE_NO_PAD.encode(id.as_bytes()));
        Ok(Self {
            id: id.to_string(),
            reference,
            name,
            mime,
            bytes: bytes.into(),
        })
    }
    // qual:allow(dry, boilerplate) reason: "Immutable upload drafts keep admitted bytes and their stable identity together."
    pub fn id(&self) -> &str {
        &self.id
    }
    // qual:allow(dry, boilerplate) reason: "Admitted upload metadata is immutable across retries."
    pub fn name(&self) -> &str {
        &self.name
    }
    // qual:allow(dry, boilerplate) reason: "Admitted MIME types cannot change after draft creation."
    pub fn mime(&self) -> &str {
        &self.mime
    }
    // qual:allow(dry, boilerplate) reason: "Bytes are borrowed without copying or allowing unvalidated mutation."
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn reference(&self) -> String {
        self.reference.clone()
    }
    /// The same file under a new upload ID, for uploading afresh after the
    /// first upload expired or went missing.
    pub fn renewed(&self) -> Self {
        let id = uuid::Uuid::new_v4();
        Self {
            id: id.to_string(),
            reference: format!("UPL-{}", URL_SAFE_NO_PAD.encode(id.as_bytes())),
            name: self.name.clone(),
            mime: self.mime.clone(),
            bytes: Arc::clone(&self.bytes),
        }
    }
}
pub fn supported(mime: &str) -> bool {
    matches!(
        mime,
        "text/markdown"
            | "text/plain"
            | "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "application/zip"
            | "application/pdf"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    )
}
pub fn mime_for_extension(extension: &str) -> Option<&'static str> {
    let mime = match extension.to_ascii_lowercase().as_str() {
        "markdown" => Some("text/markdown"),
        "text" => Some("text/plain"),
        "zip" => Some("application/zip"),
        _ => crate::uploads::file_mime(std::path::Path::new(&format!("file.{extension}"))),
    };
    mime.filter(|mime| supported(mime))
}
pub fn canonical(reference: &str) -> Option<String> {
    let text = reference.get(PREFIX_LEN..)?;
    let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
    (reference.get(..PREFIX_LEN)?.eq_ignore_ascii_case("UPL-")
        && text.len() == PAYLOAD_LEN
        && bytes.len() == UUID_BYTES
        && bytes.iter().any(|b| *b != 0)
        && URL_SAFE_NO_PAD.encode(&bytes) == text)
        .then(|| format!("UPL-{text}"))
}

pub fn references(text: &str) -> Vec<String> {
    let upper = text.to_ascii_uppercase();
    upper
        .match_indices("UPL-")
        .filter_map(|(start, _)| {
            let end = start + KEY_LEN;
            let before = start.checked_sub(1).and_then(|i| text.as_bytes().get(i));
            let after = text.as_bytes().get(end);
            if before.is_some_and(word) || after.is_some_and(word) {
                return None;
            }
            canonical(text.get(start..end)?)
        })
        .take(50)
        .collect()
}
fn word(b: &u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(*b, b'_' | b'-')
}
pub fn message(upload: &Upload, preview: &str) -> String {
    let line = preview
        .lines()
        .next()
        .unwrap_or(&upload.file_name)
        .chars()
        .filter(|c| !c.is_control())
        .scan(0, |used, c| {
            *used += c.len_utf8();
            (*used <= PREVIEW_BYTES).then_some(c)
        })
        .collect::<String>();
    format!(
        "{} — {} ({} bytes)",
        upload.reference, line, upload.size_bytes
    )
}

impl Upload {
    pub fn expired(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .is_ok_and(|at| at <= chrono::Utc::now())
    }
}

fn clean_name(name: &str) -> Result<String, UploadError> {
    use unicode_general_category::{GeneralCategory, get_general_category};
    const MAX_NAME_UTF16: usize = 200;
    let leaf = name.rsplit(['\\', '/']).next().unwrap_or_default();
    let clean = leaf
        .chars()
        .filter(|c| {
            !matches!(
                get_general_category(*c),
                GeneralCategory::Control | GeneralCategory::Format
            )
        })
        .collect::<String>();
    let clean = clean.trim();
    if clean.is_empty() || clean.encode_utf16().count() > MAX_NAME_UTF16 {
        return Err(UploadError::InvalidName);
    }
    Ok(clean.to_owned())
}
