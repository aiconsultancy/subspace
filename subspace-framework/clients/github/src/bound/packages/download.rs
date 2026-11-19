use subspace_core::http::{Error, TIMEOUT, agent, status};
use ureq::{Body, http::Response};
use url::Url;

use super::super::BoundRead;
use super::errors::{self, Read};
use crate::transport;

const REDIRECT_LIMIT: usize = 5;

impl BoundRead {
    pub(super) fn package_request(
        &self,
        path: &str,
        accept: &str,
        read: Read,
    ) -> Result<Response<Body>, Error> {
        let url = format!("{}{}", self.endpoint, path);
        let response = request(&url, Some(&self.credential), accept, read)?;
        checked(response, read)
    }

    pub(super) fn package_download(
        &self,
        path: &str,
        max: usize,
        accept: &str,
    ) -> Result<Vec<u8>, Error> {
        let initial = Url::parse(&format!("{}{}", self.endpoint, path)).map_err(|_| failed())?;
        let mut response = request(
            initial.as_str(),
            Some(&self.credential),
            accept,
            Read::DOWNLOAD,
        )?;
        let mut current = initial.clone();
        for _ in 0..REDIRECT_LIMIT {
            if response.status().is_success() {
                return transport::read_limited(response, max)
                    .map_err(|error| errors::body(error, Read::DOWNLOAD));
            }
            if !response.status().is_redirection() {
                return Err(errors::rejected(response.status().as_u16(), Read::DOWNLOAD));
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(failed)?;
            current = current.join(location).map_err(|_| failed())?;
            redirect_allowed(&initial, &current)?;
            // No redirect request carries Authorization, even when it returns to
            // the initial origin. Signed release URLs stay in memory only.
            response = request(
                current.as_str(),
                None,
                "application/octet-stream",
                Read::DOWNLOAD,
            )?;
        }
        Err(failed())
    }
}

fn request(
    url: &str,
    credential: Option<&subspace_core::credentials::Credential>,
    accept: &str,
    read: Read,
) -> Result<Response<Body>, Error> {
    // No request logger here: signed redirect URLs are source credentials.
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(ureq::Proxy::try_from_env())
        .tls_config(agent().config().tls_config().clone())
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut request = agent
        .get(url)
        .header("Accept", accept)
        .header("User-Agent", "subspace")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(credential) = credential {
        request = request.header("Authorization", credential.authorization().expose());
    }
    request.call().map_err(|error| errors::network(error, read))
}

fn checked(response: Response<Body>, read: Read) -> Result<Response<Body>, Error> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(errors::rejected(response.status().as_u16(), read))
    }
}

fn failed() -> Error {
    Error::Network(
        "GitHub returned an invalid package download redirect or exceeded the redirect limit."
            .into(),
    )
}

pub(super) fn redirect_allowed(initial: &Url, next: &Url) -> Result<(), Error> {
    let approved_host = matches!(
        next.host_str(),
        Some(
            "api.github.com"
                | "codeload.github.com"
                | "objects.githubusercontent.com"
                | "release-assets.githubusercontent.com"
        )
    );
    let same_origin = initial.origin() == next.origin();
    if next.scheme() != "https"
        || !next.username().is_empty()
        || next.password().is_some()
        || next.fragment().is_some()
        || next.port().is_some()
        || (!approved_host && !same_origin)
    {
        return Err(transport::status(
            status::FORBIDDEN,
            "GitHub returned an unapproved download redirect.",
        ));
    }
    Ok(())
}
