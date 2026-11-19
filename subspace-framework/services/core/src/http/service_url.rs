/// The supplied service URL was not a plain HTTPS origin and path.
#[derive(Debug, thiserror::Error)]
#[error("must be a plain https URL (no credentials, query or fragment)")]
pub struct InvalidUrl;

/// Checks a service base URL: `https`, no credentials, query or fragment.
/// Returns the origin and path without a trailing slash.
pub fn plain_https_url(value: &str) -> Result<String, InvalidUrl> {
    let url = url::Url::parse(value).map_err(|_| InvalidUrl)?;
    let plain = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if !plain {
        return Err(InvalidUrl);
    }
    Ok(format!(
        "{}{}",
        url.origin().ascii_serialization(),
        url.path().trim_end_matches('/')
    ))
}

#[cfg(test)]
mod tests {
    use super::plain_https_url;

    #[test]
    fn plain_https_urls() {
        assert_eq!(
            plain_https_url("https://Service.Example.com:443/api//")
                .as_deref()
                .map_err(ToString::to_string),
            Ok("https://service.example.com/api")
        );
        assert_eq!(
            plain_https_url("https://service.example.com")
                .map_err(|error| error.to_string())
                .as_deref(),
            Ok("https://service.example.com")
        );
        assert_eq!(
            plain_https_url("https://service.example.com:8443/")
                .map_err(|error| error.to_string())
                .as_deref(),
            Ok("https://service.example.com:8443")
        );
        for bad in [
            "",
            "service.example.com",
            "http://service.example.com",
            "https://user@service.example.com",
            "https://user:pw@service.example.com",
            "https://service.example.com/?a=1",
            "https://service.example.com/#top",
        ] {
            assert!(plain_https_url(bad).is_err(), "{bad}");
        }
    }
}
