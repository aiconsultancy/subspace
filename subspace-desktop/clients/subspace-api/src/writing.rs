//! A comment POST is attempted once. Ambiguous results must be checked by a reader.
use crate::{Client, Error, model::Comment, transport};
use ureq::http::StatusCode;

const MAX_COMMENT_LENGTH: usize = 50_000;
pub(crate) const MAX_RETRY_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct CommentIntent {
    body: String,
    parent: Option<String>,
}

impl std::fmt::Debug for CommentIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CommentIntent([redacted])")
    }
}

impl CommentIntent {
    pub fn new(body: &str, parent: Option<&str>) -> Result<Self, WriteError> {
        let body = normalized(body);
        if !valid_body(&body) {
            return Err(WriteError::InvalidBody);
        }
        if let Some(parent) = parent {
            transport::validate_segments(std::iter::once(parent))
                .map_err(|_| WriteError::InvalidBody)?;
        }
        Ok(Self {
            body,
            parent: parent.map(str::to_owned),
        })
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }
}

pub(crate) fn valid_body(body: &str) -> bool {
    !body.trim().is_empty()
        && !body.contains('\0')
        && body.encode_utf16().count() <= MAX_COMMENT_LENGTH
}

pub(crate) fn normalized(body: &str) -> String {
    body.replace("\r\n", "\n").replace('\r', "\n")
}

#[derive(Clone)]
pub enum CommentPost {
    Created(Box<Comment>),
    AlreadyPosted,
}

impl std::fmt::Debug for CommentPost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created(comment) => f.debug_tuple("Created").field(comment).finish(),
            Self::AlreadyPosted => f.write_str("AlreadyPosted"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("Enter a comment of at most 50,000 characters.")]
    InvalidBody,
    #[error("Sign in under Settings › Account, then reload Issues. Your draft is still here.")]
    SignInNeeded,
    #[error("The comment was not sent. Your draft is still here.")]
    NotSent,
    #[error("The service refused this comment ({status}). Your draft is still here.")]
    Refused { status: u16, retry_after: u64 },
    #[error("The comment may have posted. Check discussion before posting again.")]
    Unclear {
        status: Option<u16>,
        retry_after: u64,
    },
}

// qual:allow(coupling, oi) reason: "This client boundary composes request encoding, transport and safe response classification."
impl Client {
    pub fn post_comment(
        &self,
        reference: &str,
        intent: &CommentIntent,
    ) -> Result<CommentPost, WriteError> {
        let url = transport::endpoint(&self.base, &["issues", reference, "comments"], &[])
            .map_err(|_| WriteError::NotSent)?;
        let body = serde_json::json!({"body": intent.body(), "parentCommentId": intent.parent()});
        self.send_comment(&url, &body)
    }

    fn send_comment(
        &self,
        url: &url::Url,
        body: &serde_json::Value,
    ) -> Result<CommentPost, WriteError> {
        transport::prepare(url);
        let mut request = self
            .agent
            .post(url.as_str())
            .header("User-Agent", transport::USER_AGENT)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
        if let Some(jwt) = &self.access_jwt {
            request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
        }
        let response = request
            .config()
            .timeout_global(Some(transport::READ_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build()
            .send(body.to_string().as_bytes())
            .map_err(|_| WriteError::Unclear {
                status: None,
                retry_after: 0,
            })?;
        comment_response(response)
    }
}

fn comment_response(response: ureq::http::Response<ureq::Body>) -> Result<CommentPost, WriteError> {
    let status = response.status();
    let retry_after = transport::header(&response, "retry-after")
        .parse::<u64>()
        .unwrap_or(0)
        .min(MAX_RETRY_SECONDS);
    if transport::is_access_challenge(&response) || status == StatusCode::UNAUTHORIZED {
        return Err(WriteError::SignInNeeded);
    }
    if status == StatusCode::CREATED {
        return transport::read_json(response)
            .map(|row| CommentPost::Created(Box::new(row)))
            .map_err(|_: Error| WriteError::Unclear {
                status: Some(status.as_u16()),
                retry_after,
            });
    }
    if status.is_client_error() && status != StatusCode::REQUEST_TIMEOUT {
        Err(WriteError::Refused {
            status: status.as_u16(),
            retry_after,
        })
    } else {
        Err(WriteError::Unclear {
            status: Some(status.as_u16()),
            retry_after,
        })
    }
}
