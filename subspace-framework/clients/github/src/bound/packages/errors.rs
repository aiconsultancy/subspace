use subspace_core::http::{Error, status};

use crate::transport;

#[derive(Clone, Copy)]
pub(super) struct Read {
    action: &'static str,
    missing: &'static str,
}

impl Read {
    pub(super) const MEMBERSHIP: Self = Self {
        action: "verify organization membership",
        missing: "Active organization membership was not found or is not accessible to this credential.",
    };
    pub(super) const REPOSITORY: Self = Self {
        action: "verify repository read access",
        missing: "The repository was not found or this account cannot read it.",
    };
    pub(super) const REFERENCE: Self = Self {
        action: "resolve the selected branch, tag or commit",
        missing: "The selected branch, tag or commit was not found.",
    };
    pub(super) const RELEASE: Self = Self {
        action: "read the selected release",
        missing: "The selected release was not found or is not accessible to this account.",
    };
    pub(super) const DOWNLOAD: Self = Self {
        action: "download the selected package",
        missing: "The package download was not found or is not accessible to this account.",
    };
}

pub(super) fn rejected(code: u16, read: Read) -> Error {
    let Read { action, missing } = read;
    let reason = match code {
        status::UNAUTHORIZED => "GitHub rejected the selected account credential. Sign in again.",
        status::FORBIDDEN => {
            "GitHub denied access. Check the selected account and its organization permissions."
        }
        status::NOT_FOUND => missing,
        407 => "The proxy requires authentication. Check your proxy settings.",
        status::TOO_MANY_REQUESTS => "GitHub rate limit reached. Try again later.",
        status::UNSUPPORTED_MEDIA_TYPE => "GitHub rejected the requested download format.",
        code if ureq::http::StatusCode::from_u16(code).is_ok_and(|code| code.is_server_error()) => {
            "GitHub or the proxy is unavailable. Try again later."
        }
        _ => "GitHub rejected the request.",
    };
    transport::status(code, format!("Could not {action}. {reason} HTTP {code}."))
}

pub(super) fn malformed(read: Read) -> Error {
    transport::status(
        status::BAD_GATEWAY,
        format!(
            "Could not {}. GitHub returned malformed response data.",
            read.action
        ),
    )
}

pub(super) fn body(error: Error, read: Read) -> Error {
    match error {
        Error::Status {
            status: status::PAYLOAD_TOO_LARGE,
            ..
        } => transport::status(
            status::PAYLOAD_TOO_LARGE,
            format!(
                "Could not {}. The response exceeds the package size limit.",
                read.action
            ),
        ),
        _ => Error::Network(format!(
            "Could not {}. The response could not be read. Check your connection and proxy settings.",
            read.action
        )),
    }
}

pub(super) fn network(error: ureq::Error, read: Read) -> Error {
    let reason = match error {
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) | ureq::Error::TlsRequired => {
            "The TLS connection failed. Check your system certificates and proxy settings."
        }
        ureq::Error::InvalidProxyUrl | ureq::Error::ConnectProxyFailed(_) => {
            "The proxy connection failed. Check your proxy settings."
        }
        ureq::Error::Timeout(_) => "The request timed out. Check your connection and try again.",
        ureq::Error::HostNotFound => {
            "The GitHub host could not be resolved. Check your network and DNS settings."
        }
        ureq::Error::Protocol(_) | ureq::Error::LargeResponseHeader(_, _) => {
            "GitHub or the proxy returned a malformed HTTP response."
        }
        _ => "The network connection failed. Check your connection and proxy settings.",
    };
    Error::Network(format!("Could not {}. {reason}", read.action))
}

#[cfg(test)]
mod tests {
    use super::{Read, body, network, rejected};
    use subspace_core::http::{Error, status};

    #[test]
    fn network_categories_never_include_raw_transport_details() {
        for (error, expected) in [
            (ureq::Error::Tls("dummy-test-only TLS detail"), "TLS"),
            (
                ureq::Error::ConnectProxyFailed("https://dummy:fake@proxy.test/".into()),
                "proxy connection",
            ),
            (ureq::Error::HostNotFound, "DNS"),
            (ureq::Error::Timeout(ureq::Timeout::Global), "timed out"),
            (
                ureq::Error::BadUri("https://dummy:fake@example.test/".into()),
                "network connection",
            ),
            (ureq::Error::LargeResponseHeader(123, 100), "malformed HTTP"),
        ] {
            let message = network(error, Read::DOWNLOAD).to_string();
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("download the selected package"));
            assert!(!message.contains("dummy"));
            assert!(!message.contains("https://"));
        }
    }

    #[test]
    fn missing_release_and_download_errors_name_the_failed_operation() {
        for (read, expected) in [
            (Read::RELEASE, "The selected release was not found"),
            (Read::DOWNLOAD, "The package download was not found"),
        ] {
            let message = rejected(status::NOT_FOUND, read).to_string();
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("HTTP 404"));
        }
    }

    #[test]
    fn body_errors_distinguish_size_limits_from_read_failures_without_raw_details() {
        let excessive = body(
            Error::Status {
                status: status::PAYLOAD_TOO_LARGE,
                message: "dummy-test-only response detail".into(),
            },
            Read::DOWNLOAD,
        );
        assert!(excessive.to_string().contains("package size limit"));
        assert!(!excessive.to_string().contains("dummy-test-only"));
        let unreadable = body(
            Error::Network("dummy-test-only transport detail".into()),
            Read::DOWNLOAD,
        );
        assert!(
            unreadable
                .to_string()
                .contains("response could not be read")
        );
        assert!(!unreadable.to_string().contains("dummy-test-only"));
    }
}
