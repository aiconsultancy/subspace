//! Explicit development origins. Production ignores the environment seam.
use url::{Host, Url};

#[cfg(debug_assertions)]
pub(crate) fn read_access_jwt(
    path: Option<&std::path::Path>,
) -> Result<Option<subspace_framework::core::Secret>, crate::Error> {
    path.map(|path| {
        let text = std::fs::read_to_string(path).map_err(|_| crate::Error::InvalidRequest)?;
        let jwt = text.trim_end_matches(['\r', '\n']);
        if jwt.is_empty() || jwt.contains(['\r', '\n']) {
            return Err(crate::Error::InvalidRequest);
        }
        Ok(subspace_framework::core::Secret::new(jwt))
    })
    .transpose()
}

pub fn fixture_from_env(debug: bool, api: Option<&str>) -> Option<Url> {
    debug
        .then_some(api)
        .flatten()
        .and_then(|value| loopback_url(value).ok())
}

pub(crate) fn loopback_url(value: &str) -> Result<Url, crate::Error> {
    let url = Url::parse(value).map_err(|_| crate::Error::InvalidRequest)?;
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    let valid = loopback
        && matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if valid {
        Ok(url)
    } else {
        Err(crate::Error::InvalidRequest)
    }
}
