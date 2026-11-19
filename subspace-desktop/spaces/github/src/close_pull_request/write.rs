//! Ordered, single-attempt writes and authored failure messages.

use subspace_framework::client_github::Client;
use subspace_framework::core::http::Error;

pub(super) struct Outcome {
    pub(super) posted: bool,
    pub(super) result: Result<(), WriteError>,
}

pub(super) enum WriteError {
    Comment(Error),
    Close(Error),
}

/// Each confirmation makes one attempt; a posted comment is never sent again.
pub(super) fn write(client: &Client, login: &str, key: &str, text: &str, posted: bool) -> Outcome {
    let comment = comment_to_post(text, posted).map_or(Ok(posted), |text| {
        client.issue_comment(login, key, text).map(|()| true)
    });
    match comment {
        Ok(posted) => Outcome {
            posted,
            result: client
                .close_pull_request(login, key)
                .map_err(WriteError::Close),
        },
        Err(error) => Outcome {
            posted,
            result: Err(WriteError::Comment(error)),
        },
    }
}

pub(super) fn comment_to_post(text: &str, posted: bool) -> Option<&str> {
    (!posted && !text.trim().is_empty()).then_some(text)
}

pub(super) fn failure_message(posted: bool, failure: &WriteError) -> String {
    let error = match failure {
        WriteError::Comment(error) | WriteError::Close(error) => error,
    };
    let prefix = if posted {
        "The comment was posted. The pull request is still open. Try closing it again; the comment will not be posted again."
    } else if matches!(failure, WriteError::Comment(_)) {
        "The comment was not posted, and the pull request was not closed. Your text is kept. Try again when ready."
    } else {
        "The pull request was not closed. Check your access and connection, then try again."
    };
    // Never expose server response text, which can contain internal details.
    let reason = match error {
        Error::Credential(_) => {
            "No GitHub account is signed in for this board. Sign in with gh, then try again."
        }
        Error::Network(_) => "GitHub could not be reached.",
        Error::Status { status: 401, .. } => "GitHub rejected the account credentials.",
        _ => {
            "GitHub did not accept the request. Check repository permissions and the pull request on GitHub."
        }
    };
    format!("{prefix} {reason}")
}
