//! NETT chat uploads, for pastes and files in Channels: the draft's UUID is
//! the request key and becomes the upload's ID, so its `UPL-` reference is
//! known before the upload and an uncertain upload is found again by it.
//! The bytes go once to the prepared upload URL without credentials, and a
//! file is read through a short-lived grant, also without credentials.

use serde_json::{Value, json};
use std::time::Duration;

use super::transfer::{self, Declaration, Expected, TransferError};
use crate::chat_uploads::{Card, MAX_BYTES, Upload, UploadDraft, UploadError};
use crate::{Client, Error};

fn chat_error(error: TransferError) -> UploadError {
    match error {
        TransferError::SignIn => UploadError::Access,
        TransferError::Forbidden => UploadError::Forbidden,
        TransferError::Missing => UploadError::Missing,
        TransferError::Conflict => UploadError::Conflict,
        TransferError::Invalid => UploadError::Invalid,
        TransferError::Busy(_) | TransferError::Network => UploadError::Unavailable,
        TransferError::Unclear => UploadError::Unconfirmed,
    }
}

/// The key a chat draft's kept wait is held under.
fn held_draft(draft: &UploadDraft) -> String {
    format!("chat:{}", draft.id())
}

/// Refuses a check while a wait NETT gave for the draft's completion
/// lasts: nothing is read, prepared or completed before then.
pub(crate) fn chat_wait(draft: &UploadDraft) -> Result<(), UploadError> {
    match transfer::wait_left(&held_draft(draft)) {
        Some(_) => Err(UploadError::Unavailable),
        None => Ok(()),
    }
}

fn sleep(delay: Duration) {
    std::thread::sleep(delay);
}

// qual:allow(coupling, oi) reason: "The NETT chat boundary composes reservations, the credential-free file host and grant reads."
impl Client {
    fn nett_chat_prepare(&self, draft: &UploadDraft) -> Result<Value, TransferError> {
        let body = Declaration {
            request_key: draft.id(),
            file_name: draft.name(),
            content_type: draft.mime(),
            bytes: draft.bytes(),
        }
        .body();
        transfer::write(
            self,
            "POST",
            &["chat", "uploads", "prepare"],
            &[],
            Some(&body),
        )?
        .ok_or(TransferError::Unclear)
    }

    /// Completes the draft's transfer. A wait NETT asks for is kept for the
    /// draft too, so a check before it ends sends nothing at all.
    fn nett_chat_complete(
        &self,
        draft: &UploadDraft,
        upload: &Value,
    ) -> Result<Upload, TransferError> {
        let upload_id = upload
            .get("uploadId")
            .and_then(Value::as_str)
            .ok_or(TransferError::Unclear)?;
        let record =
            transfer::complete(self, &["chat", "uploads", "from-upload"], upload_id, &sleep)
                .inspect_err(|error| {
                    if let TransferError::Busy(wait) = error {
                        transfer::hold_until(&held_draft(draft), *wait);
                    }
                })?;
        serde_json::from_value(record).map_err(|_| TransferError::Unclear)
    }

    /// Prepare, one `PUT` and completion. A reservation whose bytes did not
    /// arrive is kept, so trying again reuses it under the same key.
    pub(crate) fn nett_create_chat_upload(
        &self,
        draft: &UploadDraft,
    ) -> Result<Upload, UploadError> {
        let upload = self.nett_chat_prepare(draft).map_err(|error| match error {
            // Nothing was sent; the same key can be used again.
            TransferError::Unclear => UploadError::Unavailable,
            error => chat_error(error),
        })?;
        // The upload URL is redeemed only for the account and organisation
        // that prepared it.
        self.current().map_err(|_| UploadError::Access)?;
        match transfer::put(&upload, &self.base, draft.mime(), draft.bytes()) {
            // Once the bytes may be stored, every failure is checked before
            // anything is sent again, a refused sign-in included: the check
            // completes the same transfer.
            Ok(()) => self
                .nett_chat_complete(draft, &upload)
                .map_err(|_| UploadError::Unconfirmed),
            Err(TransferError::Unclear) => match self.nett_chat_complete(draft, &upload) {
                Ok(upload) => Ok(upload),
                // The bytes never arrived: the reservation waits for a retry.
                Err(TransferError::Conflict) => Err(UploadError::Unavailable),
                Err(_) => Err(UploadError::Unconfirmed),
            },
            Err(error) => Err(chat_error(error)),
        }
    }

    /// The draft's upload by its reference, completing a reservation whose
    /// bytes arrived but whose completion went unanswered.
    pub(crate) fn nett_check_chat_upload(
        &self,
        draft: &UploadDraft,
    ) -> Result<Upload, UploadError> {
        match self.nett_chat_upload(&draft.reference()) {
            Err(UploadError::Missing) => {}
            other => return other,
        }
        let upload = match self.nett_chat_prepare(draft) {
            Ok(upload) => upload,
            Err(TransferError::Conflict) => return Err(UploadError::Missing),
            Err(error) => return Err(chat_error(error)),
        };
        self.nett_chat_complete(draft, &upload)
            .map_err(|error| match error {
                TransferError::Conflict => UploadError::Missing,
                error => chat_error(error),
            })
    }

    pub(crate) fn nett_chat_upload(&self, reference: &str) -> Result<Upload, UploadError> {
        let value = self
            .nett_value(&["chat", "uploads", reference], &[])
            .map_err(UploadError::from)?;
        serde_json::from_value(value).map_err(|_| UploadError::Unavailable)
    }

    /// The pasted reference as a card. NETT answers a neutral 404 for an
    /// unknown, deleted or expired upload, shown as missing.
    pub(crate) fn nett_resolve_chat_upload(&self, reference: &str) -> Result<Card, UploadError> {
        let upload = match self.nett_chat_upload(reference) {
            Ok(upload) => Some(upload),
            Err(UploadError::Missing) => None,
            Err(error) => return Err(error),
        };
        let card = json!({
            "reference": reference,
            "kind": "chatUpload",
            "status": if upload.is_some() { "available" } else { "missing" },
            "upload": null,
        });
        let mut card: Card = serde_json::from_value(card).map_err(|_| UploadError::Unavailable)?;
        card.upload = upload;
        Ok(card)
    }

    /// The file's bytes through a fresh grant: the exact stored type, its
    /// declared length and `nosniff`, from a host that never sees the
    /// bearer.
    pub(crate) fn nett_chat_upload_bytes(&self, upload: &Upload) -> Result<Vec<u8>, UploadError> {
        let grant = transfer::write(
            self,
            "POST",
            &["chat", "uploads", &upload.reference, "grants"],
            &[],
            None,
        )
        .map_err(chat_error)?
        .ok_or(UploadError::Unavailable)?;
        let (url, _) = self
            .grant_parts(&grant)
            .map_err(|_| UploadError::Unavailable)?;
        self.current().map_err(|_| UploadError::Access)?;
        let bytes = transfer::fetch(
            &url,
            &Expected {
                content_type: &upload.content_type,
                size: upload.size_bytes as u64,
                limit: MAX_BYTES,
                strict: true,
            },
        )
        .map_err(|error| match error {
            Error::NotFound => UploadError::Missing,
            _ => UploadError::Unavailable,
        })?;
        self.current().map_err(|_| UploadError::Access)?;
        Ok(bytes)
    }
}
