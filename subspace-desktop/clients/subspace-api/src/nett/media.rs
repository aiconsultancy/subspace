//! NETT media. Two bindings with their own routes:
//!
//! - managed media in issue descriptions and comments: images and files
//!   read through the media's own grant, and comment uploads through an
//!   `issueCommentDraft` reservation that the comment claims by its
//!   `mediaDraftId`;
//! - standalone attachments (the `attachments` feature): listed per issue,
//!   uploaded by prepare, one credential-free `PUT` and completion, removed
//!   with their version, and read through file grants.
//!
//! Grants are capabilities. They are cached per API origin, organisation,
//! account and file, used only while the client's lease is current, and
//! never logged or shown.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use ureq::http::StatusCode;
use url::Url;

use super::transfer::{self, Declaration, Expected, TransferError};
use super::wire;
use crate::media::ImageMetadata;
use crate::model::{IssueMedia, MediaBinding, Page};
use crate::uploads::{ImageDeclaration, UploadError};
use crate::{Client, Error, PageRequest};

/// A grant this close to expiry is renewed instead of used.
const RENEW_BEFORE_SECONDS: i64 = 30;
const MAX_IMAGE_PAGE: u32 = 100;
const MAX_ATTACHMENT_PAGE: u32 = 100;
/// Reads of a managed upload's state while NETT checks its bytes.
const MAX_STATE_POLLS: usize = 10;
/// Pages of an issue's attachments read to find a stored upload.
const MAX_STORED_PAGES: usize = 20;

#[derive(Clone, PartialEq, Eq, Hash)]
struct GrantKey {
    origin: String,
    org: String,
    account: u64,
    standalone: bool,
    id: String,
}

static GRANTS: Mutex<Option<HashMap<GrantKey, (Url, DateTime<Utc>)>>> = Mutex::new(None);

fn grants() -> std::sync::MutexGuard<'static, Option<HashMap<GrantKey, (Url, DateTime<Utc>)>>> {
    GRANTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Drops every cached grant: on sign-out and when the organisation changes.
pub(crate) fn forget_grants() {
    *grants() = None;
}

fn cached(key: &GrantKey) -> Option<Url> {
    let mut table = grants();
    let table = table.get_or_insert_with(HashMap::new);
    // Another account or organisation on this origin retires the rest.
    table.retain(|other, _| {
        other.origin != key.origin || (other.org == key.org && other.account == key.account)
    });
    let (url, expires) = table.get(key)?;
    (expires.signed_duration_since(Utc::now()).num_seconds() > RENEW_BEFORE_SECONDS)
        .then(|| url.clone())
}

fn remember(key: GrantKey, url: Url, expires: DateTime<Utc>) {
    grants()
        .get_or_insert_with(HashMap::new)
        .insert(key, (url, expires));
}

/// Grants cached for `origin`, for tests of their lifetime.
#[cfg(test)]
pub(crate) fn cached_for_test(origin: &str) -> usize {
    grants().as_ref().map_or(0, |table| {
        table.keys().filter(|key| key.origin == origin).count()
    })
}

/// How long a grant cached by a test lasts.
#[cfg(test)]
const TEST_GRANT_MINUTES: i64 = 5;

/// Caches a grant for `origin`, for tests of its lifetime.
#[cfg(test)]
pub(crate) fn remember_for_test(origin: &str, account: u64) -> Result<(), url::ParseError> {
    remember(
        GrantKey {
            origin: origin.into(),
            org: "org".into(),
            account,
            standalone: true,
            id: "file".into(),
        },
        Url::parse("https://files.fixture.example.test/f?grant=g")?,
        Utc::now() + chrono::Duration::minutes(TEST_GRANT_MINUTES),
    );
    Ok(())
}

fn forget(key: &GrantKey) {
    if let Some(table) = grants().as_mut() {
        table.remove(key);
    }
}

fn expiry(grant: &Value) -> Result<DateTime<Utc>, Error> {
    grant
        .get("expiresAtUtc")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .ok_or(Error::InvalidResponse)
}

pub(crate) fn read_error(error: TransferError) -> Error {
    match error {
        TransferError::SignIn => Error::SignInNeeded,
        TransferError::Forbidden => Error::Forbidden,
        TransferError::Missing => Error::NotFound,
        TransferError::Invalid => Error::InvalidRequest,
        TransferError::Busy(_) => Error::UnexpectedStatus(StatusCode::TOO_MANY_REQUESTS.as_u16()),
        TransferError::Conflict => Error::UnexpectedStatus(StatusCode::CONFLICT.as_u16()),
        TransferError::Unclear | TransferError::Network => Error::Network,
    }
}

/// An upload refused with `status`, with the server's wait in whole seconds.
fn refused(status: StatusCode, retry_after: u64) -> UploadError {
    UploadError::Refused {
        status: status.as_u16(),
        retry_after,
    }
}

/// An upload failure before any byte left: nothing to check.
fn before_bytes(error: TransferError) -> UploadError {
    match error {
        TransferError::SignIn => UploadError::SignInNeeded,
        TransferError::Forbidden => refused(StatusCode::FORBIDDEN, 0),
        TransferError::Missing => UploadError::Removed,
        TransferError::Conflict => refused(StatusCode::CONFLICT, 0),
        TransferError::Invalid => UploadError::InvalidImage,
        TransferError::Busy(seconds) => refused(StatusCode::TOO_MANY_REQUESTS, seconds),
        TransferError::Unclear | TransferError::Network => UploadError::Network,
    }
}

/// An upload failure once bytes may have been stored: check, never resend.
/// A refused sign-in is no exception; the check completes the transfer once
/// the person is signed in again. A refusal that no check can change, a
/// rejected upload (400) or a forbidden one (403), is final instead.
fn after_bytes(error: TransferError) -> UploadError {
    match error {
        TransferError::Busy(seconds) => UploadError::Unclear {
            retry_after: seconds,
        },
        TransferError::Invalid => UploadError::InvalidImage,
        TransferError::Forbidden => refused(StatusCode::FORBIDDEN, 0),
        _ => UploadError::Checking,
    }
}

fn sleep(delay: Duration) {
    std::thread::sleep(delay);
}

/// What one page of attachment records says of an upload.
#[derive(Debug, PartialEq)]
enum Stored {
    /// The record completed from the upload's transfer.
    Found(Value),
    /// A record that may be this upload's: one that names no transfer and
    /// whose fields do not show another file, which may be an identical
    /// older file or this upload's described incompletely, or one naming
    /// this transfer, or a malformed one, that cannot be read as this
    /// upload's.
    Unknown,
    /// No record on the page can be this upload's.
    Absent,
}

/// Finds the record completed from transfer `upload_id`. A record naming
/// another transfer is someone else's file however identical it is. One
/// that names none, its `uploadId` missing or `null` because NETT cannot
/// prove the transfer that created it, is never a match either, but it may
/// still be this upload's, so it counts as absence only when its own
/// fields show another file: an incomplete record, missing its type or
/// digest, is no proof. A record naming this transfer is this upload's even
/// when it cannot be read or differs from the file sent, so it never counts
/// as absence either: sending again could store the file twice.
fn stored_among<'a>(
    items: impl IntoIterator<Item = &'a Value>,
    upload_id: &str,
    declaration: &ImageDeclaration,
) -> Stored {
    let mut found = Stored::Absent;
    for item in items {
        let identical = || {
            serde_json::from_value::<IssueMedia>(item.clone())
                .is_ok_and(|media| declaration.matches(&media))
        };
        match item.get("uploadId") {
            Some(Value::String(id)) if id == upload_id => {
                return if identical() {
                    Stored::Found(item.clone())
                } else {
                    Stored::Unknown
                };
            }
            Some(Value::String(_)) => {}
            None | Some(Value::Null) if !another_file(item, declaration) => {
                found = Stored::Unknown;
            }
            None | Some(Value::Null) => {}
            // A transfer named in a shape NETT does not use.
            Some(_) => found = Stored::Unknown,
        }
    }
    found
}

/// Whether `record`'s own fields show it is another file than
/// `declaration`: a name, size or SHA-256 it gives that differs. Fields it
/// leaves out, or gives in another shape, show nothing either way.
fn another_file(record: &Value, declaration: &ImageDeclaration) -> bool {
    declaration.contradicted_by(
        record.get("fileName").and_then(Value::as_str),
        record.get("sizeBytes").and_then(Value::as_i64),
        record.get("sha256").and_then(Value::as_str),
    )
}

/// The standalone attachment `record` stored on `issue`, if it is the file
/// declared; anything else leaves the upload to be checked.
fn standalone(
    issue: &str,
    record: Value,
    declaration: &ImageDeclaration,
) -> Result<IssueMedia, UploadError> {
    let mut media: IssueMedia =
        serde_json::from_value(record).map_err(|_| UploadError::Checking)?;
    media.binding = MediaBinding::Standalone {
        issue: issue.to_owned(),
    };
    declaration
        .matches(&media)
        .then_some(media)
        .ok_or(UploadError::Checking)
}

/// The next page of an attachment list, if the reply names one. `Err` when
/// the reply cannot show whether the list ends here.
fn next_page(reply: &Value) -> Result<Option<String>, ()> {
    match reply.get("nextCursor") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(cursor)) if cursor.is_empty() => Ok(None),
        Some(Value::String(cursor)) => Ok(Some(cursor.clone())),
        Some(_) => Err(()),
    }
}

// qual:allow(coupling, oi) reason: "The NETT media boundary composes authenticated reservations and grants with the credential-free file transport."
impl Client {
    /// The issue's images, from descriptions and comments.
    pub(crate) fn nett_images(
        &self,
        issue: &str,
        page: &PageRequest,
    ) -> Result<Page<ImageMetadata>, Error> {
        let reply = self.nett_value(
            &["issues", issue, "images"],
            &page.parameters(MAX_IMAGE_PAGE)?,
        )?;
        let issue_id = reply
            .get("issueId")
            .and_then(Value::as_str)
            .unwrap_or(issue);
        let items = reply
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| wire::image(issue_id, item))
            .map(serde_json::from_value)
            .collect::<Result<Vec<ImageMetadata>, _>>()
            .map_err(|_| Error::InvalidResponse)?;
        Ok(Page {
            items,
            next_cursor: reply
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    /// The issue's standalone attachments where NETT serves them, and
    /// otherwise the description's file media.
    pub(crate) fn nett_attachments(
        &self,
        issue: &str,
        page: &PageRequest,
    ) -> Result<Page<IssueMedia>, Error> {
        if self.nett_serves(crate::backend::Feature::Attachments)? {
            let reply = self.nett_value(
                &["issues", issue, "attachments"],
                &page.parameters(MAX_ATTACHMENT_PAGE)?,
            )?;
            let mut page: Page<IssueMedia> =
                serde_json::from_value(reply).map_err(|_| Error::InvalidResponse)?;
            for media in &mut page.items {
                media.binding = MediaBinding::Standalone {
                    issue: issue.to_owned(),
                };
            }
            return Ok(page);
        }
        let mut detail = self.nett_value(&["issues", issue], &[])?;
        wire::normalise(&mut detail);
        let media: Vec<IssueMedia> = detail
            .get("media")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| Error::InvalidResponse)?
            .unwrap_or_default();
        Ok(Page {
            items: media
                .into_iter()
                .filter(|media| media.kind == "file")
                .map(|mut media| {
                    media.binding = MediaBinding::Managed;
                    media
                })
                .collect(),
            next_cursor: None,
        })
    }

    /// A media file's bytes through its grant, read only while this client's
    /// account and organisation are current.
    pub(crate) fn nett_media_bytes(
        &self,
        media: &IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        if media.size_bytes <= 0 || media.size_bytes as u64 > limit as u64 {
            return Err(Error::InvalidRequest);
        }
        self.current()?;
        let standalone = matches!(media.binding, MediaBinding::Standalone { .. });
        let key = GrantKey {
            origin: self.base.to_string(),
            org: self.org.clone().unwrap_or_default(),
            account: self.account_key(),
            standalone,
            id: if standalone {
                media.public_file_id.clone()
            } else {
                media.media_id.clone()
            },
        };
        if key.id.is_empty() {
            return Err(Error::NotFound);
        }
        let url = match cached(&key) {
            // A kept grant is redeemed only while its account and
            // organisation are still current.
            Some(url) => {
                self.current()?;
                url
            }
            None => {
                let (url, expires) = if standalone {
                    self.file_grant(&key.id)?
                } else {
                    self.media_grant(&key.id)?
                };
                // A grant that arrived after the account or organisation
                // changed is neither kept nor redeemed.
                self.current()?;
                remember(key.clone(), url.clone(), expires);
                url
            }
        };
        let result = transfer::fetch(
            &url,
            &Expected {
                content_type: &media.content_type,
                size: media.size_bytes as u64,
                limit,
                strict: standalone,
            },
        );
        if result.is_err() {
            forget(&key);
        }
        // The account may have changed while the bytes arrived.
        self.current()?;
        result
    }

    /// A managed media file's grant, from its state.
    fn media_grant(&self, media: &str) -> Result<(Url, DateTime<Utc>), Error> {
        let state = self.nett_value(&["issues", "media", media], &[])?;
        if state.get("status").and_then(Value::as_str) != Some("ready") {
            return Err(Error::NotFound);
        }
        let grant = state.get("grant").ok_or(Error::NotFound)?;
        let url = transfer::file_url(
            grant.get("url").and_then(Value::as_str).unwrap_or_default(),
            &self.base,
        )?;
        Ok((url, expiry(grant)?))
    }

    /// A standalone file's grant, from the organisation's file grants.
    pub(crate) fn file_grant(&self, public_file_id: &str) -> Result<(Url, DateTime<Utc>), Error> {
        let reply = transfer::write(
            self,
            "POST",
            &["files", "grants"],
            &[],
            Some(&json!({"publicFileIds": [public_file_id]})),
        )
        .map_err(read_error)?
        .ok_or(Error::InvalidResponse)?;
        let grant = reply
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|grant| grant.get("publicFileId").and_then(Value::as_str) == Some(public_file_id))
            .ok_or(Error::InvalidResponse)?;
        self.grant_parts(grant)
    }

    pub(crate) fn grant_parts(&self, grant: &Value) -> Result<(Url, DateTime<Utc>), Error> {
        let url = transfer::file_url(
            grant.get("url").and_then(Value::as_str).unwrap_or_default(),
            &self.base,
        )?;
        Ok((url, expiry(grant)?))
    }

    /// One standalone attachment: prepare, one credential-free `PUT`, then
    /// completion, waiting while NETT checks the bytes. `request_key` names
    /// the reservation. The attempt says which transfer it reserved and
    /// whether its bytes may have been stored, so a failure after that is
    /// resolved by completing the same transfer, never by sending again.
    pub(crate) fn nett_upload_attachment(
        &self,
        issue: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> crate::uploads::AttachmentAttempt {
        let mut attempt = crate::uploads::AttachmentAttempt::default();
        attempt.result =
            self.standalone_upload(issue, request_key, declaration, bytes, &mut attempt);
        attempt
    }

    fn standalone_upload(
        &self,
        issue: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
        attempt: &mut crate::uploads::AttachmentAttempt,
    ) -> Result<IssueMedia, UploadError> {
        if !self
            .nett_serves(crate::backend::Feature::Attachments)
            .unwrap_or(false)
        {
            return Err(refused(StatusCode::NOT_IMPLEMENTED, 0));
        }
        let upload = self.attachment_reservation(issue, request_key, declaration, bytes)?;
        let upload_id = upload
            .get("uploadId")
            .and_then(Value::as_str)
            .ok_or(UploadError::Network)?
            .to_owned();
        attempt.transfer = Some(upload_id.clone());
        // The upload URL is redeemed only for the account and organisation
        // that prepared it.
        self.current().map_err(|_| UploadError::SignInNeeded)?;
        match transfer::put(&upload, &self.base, &declaration.content_type, bytes) {
            Ok(()) | Err(TransferError::Unclear) => attempt.bytes_sent = true,
            Err(error) => return Err(before_bytes(error)),
        }
        match self.complete_attachment(issue, &upload_id) {
            Ok(record) => standalone(issue, record, declaration),
            Err(error) => Err(after_bytes(error)),
        }
    }

    /// Completes a standalone transfer whose bytes may have been stored:
    /// the same completion again, which replays the stored record if it
    /// already happened. A 409 or 404 proves nothing on its own: NETT also
    /// answers so when the issue changed or was archived after the upload
    /// was stored. So the issue's attachments are read, each naming the
    /// transfer it was completed from: the record from this transfer was
    /// stored; `None` means a complete read shows it was not. Anything less,
    /// such as an identical file whose transfer is not named, leaves the
    /// outcome unknown, to be checked again.
    pub(crate) fn nett_complete_attachment(
        &self,
        issue: &str,
        upload_id: &str,
        declaration: &ImageDeclaration,
    ) -> Result<Option<IssueMedia>, UploadError> {
        match self.complete_attachment(issue, upload_id) {
            Ok(record) => standalone(issue, record, declaration).map(Some),
            Err(TransferError::Conflict | TransferError::Missing) => {
                self.stored_attachment(issue, upload_id, declaration)
            }
            Err(error) => Err(after_bytes(error)),
        }
    }

    /// The issue's attachment completed from transfer `upload_id`, read
    /// across every page. `None` only when the whole list was read and no
    /// record could be this upload's.
    fn stored_attachment(
        &self,
        issue: &str,
        upload_id: &str,
        declaration: &ImageDeclaration,
    ) -> Result<Option<IssueMedia>, UploadError> {
        if !self
            .nett_serves(crate::backend::Feature::Attachments)
            .unwrap_or(false)
        {
            return Err(UploadError::Checking);
        }
        let mut page = PageRequest::default();
        let mut unknown = false;
        for _ in 0..MAX_STORED_PAGES {
            let reply = self
                .nett_value(
                    &["issues", issue, "attachments"],
                    &page
                        .parameters(MAX_ATTACHMENT_PAGE)
                        .map_err(|_| UploadError::Checking)?,
                )
                .map_err(|_| UploadError::Checking)?;
            // A reply without its list proves nothing about the upload.
            let items = reply
                .get("items")
                .and_then(Value::as_array)
                .ok_or(UploadError::Checking)?;
            match stored_among(items, upload_id, declaration) {
                Stored::Found(record) => {
                    return standalone(issue, record, declaration).map(Some);
                }
                Stored::Unknown => unknown = true,
                Stored::Absent => {}
            }
            match next_page(&reply).map_err(|()| UploadError::Checking)? {
                Some(cursor) => page.cursor = Some(cursor),
                None if unknown => return Err(UploadError::Checking),
                None => return Ok(None),
            }
        }
        Err(UploadError::Checking)
    }

    /// Cancels a standalone reservation that was not completed; best effort.
    pub(crate) fn nett_cancel_attachment(&self, issue: &str, upload_id: &str) {
        let _ = transfer::write(
            self,
            "DELETE",
            &["issues", issue, "attachments", "pending", upload_id],
            &[],
            None,
        );
    }

    fn attachment_reservation(
        &self,
        issue: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> Result<Value, UploadError> {
        let body = Declaration {
            request_key,
            file_name: &declaration.file_name,
            content_type: &declaration.content_type,
            bytes,
        }
        .body();
        transfer::write(
            self,
            "POST",
            &["issues", issue, "attachments", "prepare"],
            &[],
            Some(&body),
        )
        .map_err(before_bytes)?
        .ok_or(UploadError::Network)
    }

    fn complete_attachment(&self, issue: &str, upload_id: &str) -> Result<Value, TransferError> {
        transfer::complete(
            self,
            &["issues", issue, "attachments", "from-upload"],
            upload_id,
            &sleep,
        )
    }

    /// Removes a NETT media file the way its binding requires.
    pub(crate) fn nett_delete_media(&self, media: &IssueMedia) -> Result<(), UploadError> {
        let result = match &media.binding {
            MediaBinding::Standalone { issue } => transfer::write(
                self,
                "DELETE",
                &["issues", issue, "attachments", &media.media_id],
                &[("version".into(), media.version.to_string())],
                None,
            ),
            _ => transfer::write(
                self,
                "PUT",
                &["issues", "media", &media.media_id, "upload"],
                &[],
                Some(&json!({"version": media.version, "cancel": true})),
            ),
        };
        match result {
            Ok(_) | Err(TransferError::Missing) => Ok(()),
            Err(TransferError::SignIn) => Err(UploadError::SignInNeeded),
            // The attachment changed since it was listed: read it again.
            Err(TransferError::Conflict) => Err(refused(StatusCode::CONFLICT, 0)),
            Err(TransferError::Busy(seconds)) => {
                Err(refused(StatusCode::TOO_MANY_REQUESTS, seconds))
            }
            Err(_) => Err(UploadError::Cleanup),
        }
    }

    fn comment_reservation(
        &self,
        issue: &str,
        draft: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
    ) -> Result<Value, TransferError> {
        let issue_id = self.nett_issue_id(issue).map_err(|error| match error {
            Error::SignInNeeded => TransferError::SignIn,
            Error::Forbidden => TransferError::Forbidden,
            Error::NotFound => TransferError::Missing,
            _ => TransferError::Network,
        })?;
        let body = json!({
            "requestKey": request_key,
            "target": {"kind": "issueCommentDraft", "issueId": issue_id, "draftId": draft},
            "fileName": declaration.file_name,
            "contentType": declaration.content_type,
            "sizeBytes": declaration.size_bytes,
            "sha256": declaration.sha256,
        });
        transfer::write(
            self,
            "POST",
            &["issues", "media", "prepare"],
            &[],
            Some(&body),
        )?
        .ok_or(TransferError::Unclear)
    }

    /// A managed upload's media once NETT reports it ready, reading its
    /// state while NETT checks the bytes.
    fn settle_comment_media(
        &self,
        mut state: Value,
        declaration: &ImageDeclaration,
    ) -> Result<Option<IssueMedia>, TransferError> {
        for _ in 0..MAX_STATE_POLLS {
            match state.get("status").and_then(Value::as_str) {
                Some("ready") => {
                    let mut descriptor =
                        state.get("media").cloned().ok_or(TransferError::Unclear)?;
                    if descriptor.get("sha256").is_none_or(Value::is_null) {
                        descriptor["sha256"] = json!(declaration.sha256);
                    }
                    let mut media: IssueMedia =
                        serde_json::from_value(descriptor).map_err(|_| TransferError::Unclear)?;
                    media.binding = MediaBinding::Managed;
                    return Ok(declaration.matches(&media).then_some(media));
                }
                Some("checking") => {
                    sleep(Duration::from_secs(1));
                    let upload_id = state
                        .get("uploadId")
                        .and_then(Value::as_str)
                        .ok_or(TransferError::Unclear)?
                        .to_owned();
                    let url = self
                        .endpoint(&["issues", "media", "uploads", &upload_id], &[])
                        .map_err(|_| TransferError::Unclear)?;
                    state = crate::transport::get(&self.agent, &url, self.access_jwt.as_ref())
                        .map_err(|error| match error {
                            Error::SignInNeeded => TransferError::SignIn,
                            Error::NotFound => TransferError::Missing,
                            _ => TransferError::Unclear,
                        })?;
                    // A state read for an account or organisation that is
                    // no longer current is not used.
                    self.current().map_err(|_| TransferError::SignIn)?;
                }
                _ => return Ok(None),
            }
        }
        Err(TransferError::Unclear)
    }

    fn complete_comment_media(&self, state: &Value) -> Result<Value, TransferError> {
        let upload_id = state
            .get("uploadId")
            .and_then(Value::as_str)
            .ok_or(TransferError::Unclear)?;
        let url = self
            .endpoint(&["issues", "media", "from-upload"], &[])
            .map_err(|_| TransferError::Unclear)?;
        let response = crate::transport::post_json(
            self,
            &url,
            &json!({"uploadId": upload_id}),
            crate::transport::READ_TIMEOUT,
        )
        .map_err(|_| TransferError::Unclear)?;
        match response.status() {
            StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED => {
                let state =
                    crate::transport::read_json(response).map_err(|_| TransferError::Unclear)?;
                // Read after the account or organisation changed: not used.
                self.current().map_err(|_| TransferError::SignIn)?;
                Ok(state)
            }
            status => Err(TransferError::from_status(
                status.as_u16(),
                super::flow::retry_after(&response).unwrap_or(1),
            )),
        }
    }

    /// An image or file for a comment draft, as NETT managed media the
    /// comment claims with `mediaDraftId` when it is posted. `request_key`
    /// makes a repeat of this call return the same reservation.
    pub(crate) fn nett_upload_comment_media(
        &self,
        issue: &str,
        draft: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> Result<IssueMedia, UploadError> {
        let state = self
            .comment_reservation(issue, draft, request_key, declaration)
            .map_err(before_bytes)?;
        let state = if state.get("status").and_then(Value::as_str) == Some("prepared") {
            let upload = state.get("upload").cloned().unwrap_or(Value::Null);
            // The upload URL is redeemed only for the account and
            // organisation that prepared it.
            self.current().map_err(|_| UploadError::SignInNeeded)?;
            match transfer::put(&upload, &self.base, &declaration.content_type, bytes) {
                Ok(()) | Err(TransferError::Unclear) => {
                    self.complete_comment_media(&state).map_err(after_bytes)?
                }
                Err(error) => return Err(before_bytes(error)),
            }
        } else {
            state
        };
        self.settle_comment_media(state, declaration)
            .map_err(after_bytes)?
            .ok_or(UploadError::Checking)
    }

    /// Whether an uncertain comment upload was stored: the reservation is
    /// read again by its request key and completed if its bytes arrived.
    /// `None` means nothing usable was stored.
    pub(crate) fn nett_check_comment_media(
        &self,
        issue: &str,
        draft: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
    ) -> Result<Option<IssueMedia>, UploadError> {
        let state = match self.comment_reservation(issue, draft, request_key, declaration) {
            Ok(state) => state,
            Err(TransferError::Conflict | TransferError::Missing) => return Ok(None),
            Err(error) => return Err(after_bytes(error)),
        };
        let state = if state.get("status").and_then(Value::as_str) == Some("prepared") {
            match self.complete_comment_media(&state) {
                Ok(state) => state,
                // The bytes never arrived.
                Err(TransferError::Conflict) => return Ok(None),
                Err(error) => return Err(after_bytes(error)),
            }
        } else {
            state
        };
        self.settle_comment_media(state, declaration)
            .map_err(after_bytes)
    }

    /// Cancels a comment draft's reservation, whatever state it reached.
    pub(crate) fn nett_cancel_comment_media(
        &self,
        issue: &str,
        draft: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
    ) -> Result<(), UploadError> {
        let state = match self.comment_reservation(issue, draft, request_key, declaration) {
            Ok(state) => state,
            Err(TransferError::Conflict | TransferError::Missing) => return Ok(()),
            Err(TransferError::SignIn) => return Err(UploadError::SignInNeeded),
            Err(_) => return Err(UploadError::Cleanup),
        };
        if matches!(
            state.get("status").and_then(Value::as_str),
            Some("cleaned" | "failed" | "expired") | None
        ) {
            return Ok(());
        }
        let (Some(media), Some(version)) = (
            state.get("mediaId").and_then(Value::as_str),
            state.get("version").and_then(Value::as_u64),
        ) else {
            return Err(UploadError::Cleanup);
        };
        match transfer::write(
            self,
            "PUT",
            &["issues", "media", media, "upload"],
            &[],
            Some(&json!({"version": version, "cancel": true})),
        ) {
            Ok(_) | Err(TransferError::Missing) => Ok(()),
            Err(TransferError::SignIn) => Err(UploadError::SignInNeeded),
            Err(_) => Err(UploadError::Cleanup),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GrantKey, Stored, cached, remember, stored_among};
    use chrono::{Duration, Utc};
    use url::Url;

    #[test]
    fn a_recovered_upload_is_the_record_from_its_own_transfer() {
        let bytes = b"Same file, twice.\n";
        let declaration =
            crate::uploads::ImageDeclaration::new("same.txt", "text/plain", bytes).unwrap();
        let record = |upload: serde_json::Value| {
            serde_json::json!({
                "mediaId": "m", "version": 1, "kind": "file", "fileName": "same.txt",
                "contentType": "text/plain", "sizeBytes": bytes.len(),
                "sha256": declaration.sha256, "publicFileId": "pf", "expiresAt": null,
                "uploadId": upload,
            })
        };
        let older = record(serde_json::json!("upload-older"));
        let mine = record(serde_json::json!("upload-mine"));
        // An identical file stored from another transfer is not this one.
        assert_eq!(
            stored_among([&older], "upload-mine", &declaration),
            Stored::Absent
        );
        assert_eq!(
            stored_among([&older, &mine], "upload-mine", &declaration),
            Stored::Found(mine.clone())
        );
        // A record that names no transfer leaves the outcome unknown.
        let mut unnamed = older.clone();
        unnamed.as_object_mut().unwrap().remove("uploadId");
        assert_eq!(
            stored_among([&unnamed], "upload-mine", &declaration),
            Stored::Unknown
        );
        // One whose transfer NETT cannot prove is never a match, and an
        // identical one never shows that this upload is absent.
        let unproven = record(serde_json::Value::Null);
        assert_eq!(
            stored_among([&unproven], "upload-mine", &declaration),
            Stored::Unknown
        );
        assert_eq!(
            stored_among([&unproven, &mine], "upload-mine", &declaration),
            Stored::Found(mine.clone())
        );
        // Another file without a transfer is not this upload either way.
        let mut other = unproven.clone();
        other["sha256"] = serde_json::json!("0".repeat(64));
        assert_eq!(
            stored_among([&other], "upload-mine", &declaration),
            Stored::Absent
        );
    }

    /// A NETT loopback client whose completion of `upload-unproven` is
    /// refused with 409, so the upload is looked for among `items`, the
    /// issue's whole attachment list.
    fn recovering(
        items: serde_json::Value,
    ) -> (subspace_framework::client_test_support::Mock, crate::Client) {
        let list = serde_json::json!({"items": items, "nextCursor": null}).to_string();
        let mock = subspace_framework::client_test_support::Mock::new(move |request| {
            if request.path.ends_with("/desktop/capabilities") {
                (200, r#"{"features":["attachments"]}"#.into())
            } else if request.path.ends_with("/attachments/from-upload") {
                (409, "{}".into())
            } else if request.path.contains("/attachments") {
                (200, list.clone())
            } else {
                (404, "{}".into())
            }
        });
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000d2"),
            Some(subspace_framework::core::Secret::new(
                "nett-unproven-upload",
            )),
        )
        .unwrap()
        .without_live();
        (mock, client)
    }

    #[test]
    fn an_upload_id_that_is_null_or_missing_never_shows_the_upload_absent() {
        let bytes = b"Stored without a provable transfer.\n";
        let declaration =
            crate::uploads::ImageDeclaration::new("unproven.txt", "text/plain", bytes).unwrap();
        let record = |upload: Option<serde_json::Value>, sha256: &str| {
            let mut record = serde_json::json!({
                "mediaId": "m", "version": 1, "kind": "file", "fileName": "unproven.txt",
                "contentType": "text/plain", "sizeBytes": bytes.len(),
                "sha256": sha256, "publicFileId": "pf", "expiresAt": null,
            });
            if let Some(upload) = upload {
                record["uploadId"] = upload;
            }
            record
        };
        // A whole list whose identical file names no transfer leaves the
        // upload to be checked again, never sent again.
        for upload in [Some(serde_json::Value::Null), None] {
            let (_mock, client) = recovering(serde_json::json!([record(
                upload.clone(),
                &declaration.sha256
            )]));
            assert!(
                matches!(
                    client.nett_complete_attachment("OPS-1", "upload-unproven", &declaration),
                    Err(crate::uploads::UploadError::Checking)
                ),
                "uploadId {upload:?} is unknown, not absent"
            );
        }
        // Only another file in the whole list shows the upload was not stored.
        let (_mock, client) = recovering(serde_json::json!([record(
            Some(serde_json::Value::Null),
            &"0".repeat(64)
        )]));
        assert!(matches!(
            client.nett_complete_attachment("OPS-1", "upload-unproven", &declaration),
            Ok(None)
        ));
    }

    #[test]
    fn an_incomplete_record_naming_no_transfer_is_never_absence() {
        let bytes = b"Stored, but described in part.\n";
        let declaration =
            crate::uploads::ImageDeclaration::new("part.txt", "text/plain", bytes).unwrap();
        let complete = serde_json::json!({
            "mediaId": "m", "version": 1, "kind": "file", "fileName": "part.txt",
            "contentType": "text/plain", "sizeBytes": bytes.len(),
            "sha256": declaration.sha256, "publicFileId": "pf", "expiresAt": null,
            "uploadId": null,
        });
        // Completion committed but its reply was lost, and the record lacks
        // its type, or its digest, or is not even an object: nothing shows
        // it is another file, so Retry checks rather than storing it twice.
        for missing in ["contentType", "sha256", "mediaId"] {
            let mut record = complete.clone();
            record.as_object_mut().unwrap().remove(missing);
            assert_eq!(
                stored_among([&record], "upload-mine", &declaration),
                Stored::Unknown,
                "a record without {missing} is no proof"
            );
        }
        assert_eq!(
            stored_among([&serde_json::json!("m")], "upload-mine", &declaration),
            Stored::Unknown
        );
        // What it does give can still show another file.
        let mut other = complete.clone();
        other.as_object_mut().unwrap().remove("contentType");
        other["sizeBytes"] = serde_json::json!(bytes.len() + 1);
        assert_eq!(
            stored_among([&other], "upload-mine", &declaration),
            Stored::Absent
        );
        other["sizeBytes"] = serde_json::json!(bytes.len());
        other["fileName"] = serde_json::json!("other.txt");
        assert_eq!(
            stored_among([&other], "upload-mine", &declaration),
            Stored::Absent
        );
        // Recovery across the whole list leaves the upload to be checked.
        let mut partial = complete;
        partial.as_object_mut().unwrap().remove("contentType");
        let (_mock, client) = recovering(serde_json::json!([partial]));
        assert!(matches!(
            client.nett_complete_attachment("OPS-1", "upload-unproven", &declaration),
            Err(crate::uploads::UploadError::Checking)
        ));
    }

    #[test]
    fn a_record_naming_the_transfer_that_cannot_be_read_is_never_absence() {
        let bytes = b"Stored, but described badly.\n";
        let declaration =
            crate::uploads::ImageDeclaration::new("bad.txt", "text/plain", bytes).unwrap();
        let mut mine = serde_json::json!({
            "mediaId": "m", "version": 1, "kind": "file", "fileName": "bad.txt",
            "contentType": "text/plain", "sizeBytes": bytes.len(),
            "sha256": declaration.sha256, "publicFileId": "pf", "expiresAt": null,
            "uploadId": "upload-mine",
        });
        // NETT stored the file from this transfer, but the record lost its
        // digest: the outcome stays unknown, so Retry checks, never resends.
        mine.as_object_mut().unwrap().remove("sha256");
        assert_eq!(
            stored_among([&mine], "upload-mine", &declaration),
            Stored::Unknown
        );
        // Or it describes another file.
        mine["sha256"] = serde_json::json!("0".repeat(64));
        assert_eq!(
            stored_among([&mine], "upload-mine", &declaration),
            Stored::Unknown
        );
        // A transfer named in an unexpected shape cannot be ruled out.
        mine["uploadId"] = serde_json::json!(7);
        assert_eq!(
            stored_among([&mine], "upload-mine", &declaration),
            Stored::Unknown
        );
    }

    #[test]
    fn only_a_list_that_shows_its_end_proves_an_upload_absent() {
        use super::next_page;
        use serde_json::json;
        assert_eq!(next_page(&json!({"items": []})), Ok(None));
        assert_eq!(next_page(&json!({"nextCursor": null})), Ok(None));
        assert_eq!(
            next_page(&json!({"nextCursor": "c2"})),
            Ok(Some("c2".into()))
        );
        assert_eq!(next_page(&json!({"nextCursor": 2})), Err(()));
    }

    #[test]
    fn a_recovery_read_that_is_not_a_complete_list_leaves_the_upload_unknown() {
        let mock = subspace_framework::client_test_support::Mock::new(|request| {
            if request.path.ends_with("/desktop/capabilities") {
                (200, r#"{"features":["attachments"]}"#.into())
            } else if request.path.contains("/attachments") {
                // No list at all: nothing shows the upload is not stored.
                (200, r#"{"nextCursor":null}"#.into())
            } else {
                (404, "{}".into())
            }
        });
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000d1"),
            Some(subspace_framework::core::Secret::new("nett-recovery-list")),
        )
        .unwrap()
        .without_live();
        let declaration =
            crate::uploads::ImageDeclaration::new("lost.txt", "text/plain", b"Lost reply.\n")
                .unwrap();
        assert!(matches!(
            client.stored_attachment("OPS-1", "upload-mine", &declaration),
            Err(crate::uploads::UploadError::Checking)
        ));
    }

    fn key(account: u64, org: &str) -> GrantKey {
        GrantKey {
            origin: "http://127.0.0.1:9/api".into(),
            org: org.into(),
            account,
            standalone: true,
            id: "file-1".into(),
        }
    }

    #[test]
    fn a_grant_serves_only_its_own_account_and_organisation() {
        let url = Url::parse("http://127.0.0.1:9/files/file-1?grant=g").unwrap();
        remember(key(1, "a"), url.clone(), Utc::now() + Duration::minutes(5));
        assert_eq!(cached(&key(1, "a")), Some(url.clone()));
        // Another account on the origin neither sees nor keeps it.
        assert_eq!(cached(&key(2, "a")), None);
        assert_eq!(cached(&key(1, "a")), None);
        remember(key(1, "a"), url.clone(), Utc::now() + Duration::minutes(5));
        assert_eq!(cached(&key(1, "b")), None);
        assert_eq!(cached(&key(1, "a")), None);
        // Nearly expired grants are renewed; sign-out drops the rest.
        remember(key(1, "a"), url, Utc::now() + Duration::seconds(10));
        assert_eq!(cached(&key(1, "a")), None);
    }
}
