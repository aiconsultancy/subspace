use super::{AttachmentAttempt, AttachmentProtocol, ImageDeclaration, UploadError};
use crate::{Client, media::ImageMetadata, model::IssueMedia, transport};
use std::time::Duration;
use ureq::http::StatusCode;

// Allow a 10 MB upload on a slow uplink. The write still has one send attempt.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

// qual:allow(coupling, oi) reason: "The write boundary composes bounded multipart encoding, one transport attempt and safe response classification."
impl Client {
    pub fn upload_attachment(
        &self,
        issue: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> Result<IssueMedia, UploadError> {
        if !declaration.unchanged(bytes) {
            return Err(UploadError::InvalidImage);
        }
        if self.backend == crate::Backend::Nett {
            let key = uuid::Uuid::new_v4().to_string();
            return self
                .nett_upload_attachment(issue, &key, declaration, bytes)
                .result;
        }
        self.upload_subspace_media(issue, declaration, bytes, declaration.is_image())
    }

    fn upload_subspace_media(
        &self,
        issue: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
        inline_image: bool,
    ) -> Result<IssueMedia, UploadError> {
        if !self.backend_supports_uploads() {
            return Err(UploadError::Refused {
                status: StatusCode::NOT_IMPLEMENTED.as_u16(),
                retry_after: 0,
            });
        }
        let route = if inline_image {
            "images"
        } else {
            "attachments"
        };
        let url = self
            .endpoint(&["issues", issue, route], &[])
            .map_err(|_| UploadError::Removed)?;
        let (boundary, body) = multipart(declaration, bytes);
        let response = self.media_write(
            "POST",
            &url,
            &format!("multipart/form-data; boundary={boundary}"),
            &body,
        )?;
        if inline_image {
            let metadata: ImageMetadata =
                transport::read_json(response).map_err(|_| UploadError::Checking)?;
            let media = metadata.descriptor();
            return declaration
                .matches(&media)
                .then_some(media)
                .ok_or(UploadError::Checking);
        }
        let media: IssueMedia =
            transport::read_json(response).map_err(|_| UploadError::Checking)?;
        declaration
            .matches(&media)
            .then_some(media)
            .ok_or(UploadError::Checking)
    }

    fn backend_supports_uploads(&self) -> bool {
        self.serves(crate::backend::Feature::Attachments)
    }

    /// A standalone attachment on the issue. On NETT `request_key` names the
    /// reservation, and the attempt says which transfer it reserved and
    /// whether its bytes may have been stored.
    pub fn upload_issue_attachment(
        &self,
        issue: &str,
        request_key: &str,
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> AttachmentAttempt {
        let protocol = self.attachment_protocol();
        if bytes.len() > protocol.max_bytes(&declaration.content_type)
            || !declaration.unchanged(bytes)
        {
            return AttachmentAttempt {
                result: Err(protocol.invalid_file()),
                ..AttachmentAttempt::default()
            };
        }
        match protocol {
            AttachmentProtocol::NettStandalone => {
                self.nett_upload_attachment(issue, request_key, declaration, bytes)
            }
            AttachmentProtocol::SubspaceFiles => AttachmentAttempt::direct(
                self.upload_subspace_media(issue, declaration, bytes, false),
            ),
        }
    }

    /// Completes a standalone transfer whose bytes may have been stored.
    /// `None` means it was refused for good and nothing was stored.
    pub fn complete_issue_attachment(
        &self,
        issue: &str,
        transfer: &str,
        declaration: &ImageDeclaration,
    ) -> Result<Option<IssueMedia>, UploadError> {
        if !self.attachment_protocol().checkable_uploads() {
            return Err(UploadError::Checking);
        }
        self.nett_complete_attachment(issue, transfer, declaration)
    }

    /// Cancels a standalone reservation that was not completed; best effort.
    pub fn cancel_issue_attachment(&self, issue: &str, transfer: &str) {
        if self.attachment_protocol().checkable_uploads() {
            self.nett_cancel_attachment(issue, transfer);
        }
    }

    /// An image or file for a comment draft. NETT keeps it as managed media
    /// for the draft `draft`, which the comment claims when it is posted;
    /// Subspace API stores it as an issue attachment. `request_key` lets a
    /// check find this upload again.
    pub fn upload_comment_media(
        &self,
        issue: &str,
        (draft, request_key): (&str, &str),
        declaration: &ImageDeclaration,
        bytes: &[u8],
    ) -> Result<IssueMedia, UploadError> {
        if !declaration.unchanged(bytes) {
            return Err(UploadError::InvalidImage);
        }
        if self.backend == crate::Backend::Nett {
            return self.nett_upload_comment_media(issue, draft, request_key, declaration, bytes);
        }
        self.upload_attachment(issue, declaration, bytes)
    }

    pub fn delete_media(&self, media: &IssueMedia) -> Result<(), UploadError> {
        if let AttachmentProtocol::NettStandalone = self.attachment_protocol() {
            return self.nett_delete_media(media);
        }
        if !self.backend_supports_uploads() {
            return Err(UploadError::Cleanup);
        }
        let (method, path) = if media.kind == "image" {
            (
                "POST",
                vec!["issues", "media", media.media_id.as_str(), "delete"],
            )
        } else {
            (
                "DELETE",
                vec!["issues", "attachments", media.media_id.as_str()],
            )
        };
        let url = self
            .endpoint(&path, &[])
            .map_err(|_| UploadError::Removed)?;
        let response = self.media_write(method, &url, "application/json", b"{}")?;
        transport::read_json::<bool>(response)
            .map(|_| ())
            .map_err(|_| UploadError::Cleanup)
    }

    fn media_write(
        &self,
        method: &str,
        url: &url::Url,
        mime: &str,
        body: &[u8],
    ) -> Result<ureq::http::Response<ureq::Body>, UploadError> {
        transport::prepare(url);
        let mut request = if method == "DELETE" {
            self.agent.delete(url.as_str()).force_send_body()
        } else {
            self.agent.post(url.as_str())
        };
        request = request
            .header("User-Agent", transport::USER_AGENT)
            .header("Accept", "application/json")
            .header("Content-Type", mime);
        if let Some(jwt) = &self.access_jwt {
            request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
        }
        let request = request
            .config()
            .timeout_global(Some(media_timeout(mime)))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build();
        let response = request.send(body).map_err(|_| UploadError::Checking)?;
        classify(response)
    }
}

impl AttachmentAttempt {
    /// A direct POST has no reservation. Only an indeterminate response
    /// leaves a failed attempt whose file might already be on the issue.
    fn direct(result: Result<IssueMedia, UploadError>) -> Self {
        Self {
            bytes_sent: matches!(
                result,
                Err(UploadError::Checking | UploadError::Unclear { .. })
            ),
            result,
            transfer: None,
        }
    }
}

fn media_timeout(mime: &str) -> Duration {
    if mime.starts_with("multipart/form-data;") {
        UPLOAD_TIMEOUT
    } else {
        transport::READ_TIMEOUT
    }
}

fn classify(
    response: ureq::http::Response<ureq::Body>,
) -> Result<ureq::http::Response<ureq::Body>, UploadError> {
    let status = response.status();
    if transport::is_access_challenge(&response) || status == StatusCode::UNAUTHORIZED {
        return Err(UploadError::SignInNeeded);
    }
    if status == StatusCode::OK || status == StatusCode::CREATED {
        return Ok(response);
    }
    if status.is_client_error() && status != StatusCode::REQUEST_TIMEOUT {
        let retry_after = transport::header(&response, "retry-after")
            .parse::<u64>()
            .unwrap_or(0)
            .min(crate::writing::MAX_RETRY_SECONDS);
        return Err(UploadError::Refused {
            status: status.as_u16(),
            retry_after,
        });
    }
    let retry_after = transport::header(&response, "retry-after")
        .parse::<u64>()
        .unwrap_or(0)
        .min(crate::writing::MAX_RETRY_SECONDS);
    if retry_after > 0 {
        Err(UploadError::Unclear { retry_after })
    } else {
        Err(UploadError::Checking)
    }
}

fn multipart(declaration: &ImageDeclaration, bytes: &[u8]) -> (String, Vec<u8>) {
    let mut boundary = format!("subspace-{}", declaration.sha256);
    while bytes
        .windows(boundary.len())
        .any(|window| window == boundary.as_bytes())
    {
        boundary.push('x');
    }
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n", declaration.file_name, declaration.content_type).into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

#[cfg(test)]
mod tests {
    use super::{UPLOAD_TIMEOUT, media_timeout};
    use crate::transport::READ_TIMEOUT;
    use std::time::Duration;

    #[test]
    fn multipart_gets_five_minutes_and_cleanup_keeps_the_read_deadline() {
        assert_eq!(
            media_timeout("multipart/form-data; boundary=fixture"),
            Duration::from_secs(300)
        );
        assert_eq!(media_timeout("application/json"), READ_TIMEOUT);
        assert!(UPLOAD_TIMEOUT > READ_TIMEOUT);
    }
}
