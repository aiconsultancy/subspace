use super::{Card, Cards, MAX_BYTES, Upload, UploadDraft, UploadError, canonical, supported};
use crate::{Client, Error, transport};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
const SHA256_HEX_LEN: usize = 64;
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

// qual:allow(coupling, oi) reason: "The private HTTP seam combines pacing, identity checks, response validation and single-attempt writes."
impl Client {
    pub fn create_chat_upload(&self, draft: &UploadDraft) -> Result<Upload, UploadError> {
        let user = self.me().map_err(UploadError::from)?.user_id;
        let body = super::encoding::create(draft).map_err(|_| UploadError::Invalid)?;
        let upload: Upload = self.chat_post_bytes(&["chat", "uploads"], &body)?;
        if !matches_draft(&upload, draft, &user)
            || self.me().map_err(|_| UploadError::Unconfirmed)?.user_id != user
        {
            return Err(UploadError::Unconfirmed);
        }
        validate(upload)
    }
    pub fn check_chat_upload(&self, draft: &UploadDraft) -> Result<Upload, UploadError> {
        let user = self.me().map_err(UploadError::from)?.user_id;
        let upload = self.chat_upload(&draft.reference())?;
        if !matches_draft(&upload, draft, &user)
            || self.me().map_err(UploadError::from)?.user_id != user
        {
            return Err(UploadError::Conflict);
        }
        Ok(upload)
    }
    pub fn chat_upload(&self, reference: &str) -> Result<Upload, UploadError> {
        let upload: Upload = self
            .get(&["chat", "uploads", reference], &[])
            .map_err(UploadError::from)?;
        validate(upload)
    }
    pub fn resolve_chat_upload(&self, reference: &str) -> Result<Card, UploadError> {
        let key = canonical(reference).ok_or(UploadError::Invalid)?;
        let mut cards: Cards = self.chat_post(&["resolve"], json!({"references":[key]}))?;
        let card = cards
            .items
            .pop()
            .filter(|c| c.reference == key && c.kind == "chatUpload")
            .ok_or(UploadError::Unavailable)?;
        if let Some(upload) = &card.upload {
            validate(upload.clone())?;
            if upload.reference != key {
                return Err(UploadError::Unavailable);
            }
        }
        Ok(card)
    }
    pub fn chat_upload_bytes(&self, upload: &Upload) -> Result<Vec<u8>, UploadError> {
        validate(upload.clone())?;
        let url = transport::endpoint(
            &self.base,
            &["chat", "uploads", &upload.reference, "content"],
            &[],
        )
        .map_err(UploadError::from)?;
        let bytes = transport::get_bytes_with_timeout(
            &self.agent,
            &url,
            self.access_jwt.as_ref(),
            (&upload.content_type, MAX_BYTES),
            UPLOAD_TIMEOUT,
        )
        .map_err(UploadError::from)?;
        if bytes.len() != upload.size_bytes || hash(&bytes) != upload.sha256 {
            return Err(UploadError::Unavailable);
        }
        Ok(bytes)
    }
    fn chat_post<T: DeserializeOwned>(&self, path: &[&str], body: Value) -> Result<T, UploadError> {
        let encoded = serde_json::to_vec(&body).map_err(|_| UploadError::Invalid)?;
        self.chat_post_bytes(path, &encoded)
    }
    fn chat_post_bytes<T: DeserializeOwned>(
        &self,
        path: &[&str],
        body: &[u8],
    ) -> Result<T, UploadError> {
        let url = transport::endpoint(&self.base, path, &[]).map_err(UploadError::from)?;
        let response = transport::post_json_bytes(
            self,
            &url,
            body,
            if path == ["chat", "uploads"] {
                UPLOAD_TIMEOUT
            } else {
                transport::READ_TIMEOUT
            },
        )
        .map_err(|_| UploadError::Unconfirmed)?;
        if transport::is_access_challenge(&response) {
            return Err(UploadError::Access);
        }
        let response = transport::check_response(response).map_err(UploadError::from)?;
        transport::read_json(response).map_err(|_| UploadError::Unconfirmed)
    }
}
fn validate(upload: Upload) -> Result<Upload, UploadError> {
    if canonical(&upload.reference).as_deref() != Some(&upload.reference)
        || uuid::Uuid::parse_str(&upload.id)
            .ok()
            .map(|id| format!("UPL-{}", URL_SAFE_NO_PAD.encode(id.as_bytes())))
            .as_ref()
            != Some(&upload.reference)
        || upload.size_bytes == 0
        || upload.size_bytes > MAX_BYTES
        || !supported(&upload.content_type)
        || upload.sha256.len() != SHA256_HEX_LEN
    {
        return Err(UploadError::Unavailable);
    }
    Ok(upload)
}
impl From<Error> for UploadError {
    fn from(error: Error) -> Self {
        match error {
            Error::SignInNeeded => Self::Access,
            Error::Forbidden => Self::Forbidden,
            Error::NotFound => Self::Missing,
            Error::UnexpectedStatus(410) => Self::Expired,
            Error::UnexpectedStatus(409) => Self::Conflict,
            Error::UnexpectedStatus(400 | 413 | 422) | Error::InvalidRequest => Self::Invalid,
            _ => Self::Unavailable,
        }
    }
}

fn matches_draft(upload: &Upload, draft: &UploadDraft, user: &str) -> bool {
    upload.id == draft.id()
        && upload.reference == draft.reference()
        && upload.uploaded_by_user_id == user
        && upload.file_name == draft.name()
        && upload.content_type == draft.mime()
        && upload.size_bytes == draft.bytes().len()
        && upload.sha256 == hash(draft.bytes())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
