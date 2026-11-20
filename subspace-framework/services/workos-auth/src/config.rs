use std::collections::BTreeSet;
use subspace_account::Error;

/// WorkOS's API origin.
pub const DEFAULT_API_URL: &str = "https://api.workos.com";

const CLIENT_LIMIT: usize = 128;
const NAME_LIMIT: usize = 64;

/// How the person signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    /// Authorisation code with PKCE and a loopback redirect.
    Browser,
    /// Device authorisation: the person confirms a code in the browser.
    Device,
}

/// Non-secret configuration of one WorkOS AuthKit public client.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) client_id: String,
    pub(crate) api: url::Url,
    pub(crate) redirect_port: u16,
    pub(crate) flow: Flow,
    resources: BTreeSet<String>,
    organization: String,
}

impl Config {
    /// The client ID from the WorkOS dashboard, such as `client_01H…`.
    pub fn new(client_id: &str) -> Result<Self, Error> {
        let client_id = client_id.trim();
        if client_id.is_empty()
            || client_id.len() > CLIENT_LIMIT
            || !client_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(Error::Unconfigured);
        }
        Ok(Self {
            client_id: client_id.into(),
            api: url::Url::parse(DEFAULT_API_URL).map_err(|_| Error::Unconfigured)?,
            redirect_port: 0,
            flow: Flow::Browser,
            resources: BTreeSet::new(),
            organization: String::new(),
        })
    }

    /// Another API origin: HTTPS, or HTTP on a loopback address for local
    /// stand-ins.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_api_url(mut self, value: &str) -> Result<Self, Error> {
        let url = url::Url::parse(value.trim()).map_err(|_| Error::Unconfigured)?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Unconfigured);
        }
        self.api = url;
        Ok(self)
    }

    /// A fixed loopback port for the redirect URI, when the WorkOS
    /// configuration lists one. `0`, the default, picks a fresh port.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_redirect_port(mut self, port: u16) -> Self {
        self.redirect_port = port;
        self
    }

    /// Sign in with WorkOS device authorisation instead of a loopback
    /// redirect: the browser shows a code the person confirms, and the app
    /// polls for the result. For clients that allow device authorisation
    /// and have no loopback redirect registered.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_device_authorization(mut self) -> Self {
        self.flow = Flow::Device;
        self
    }

    /// Label the organisation in the app's sign-in settings.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_organization(mut self, name: &str) -> Self {
        self.organization = name.into();
        self
    }

    pub fn organization_name(&self) -> &str {
        &self.organization
    }

    /// Register a service that accepts this client's access tokens. Token
    /// requests for any other name fail before network or keychain access.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_resource(mut self, name: &str) -> Result<Self, Error> {
        if name.is_empty()
            || name.len() > NAME_LIMIT
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            || !self.resources.insert(name.into())
        {
            return Err(Error::Unconfigured);
        }
        Ok(self)
    }

    pub(crate) fn check_resource(&self, name: &str) -> Result<(), Error> {
        if name.is_empty() || self.resources.contains(name) {
            Ok(())
        } else {
            Err(Error::Unconfigured)
        }
    }

    /// Whether the WorkOS API is a plain-HTTP loopback stand-in, which may
    /// hand out loopback verification pages.
    pub(crate) fn loopback(&self) -> bool {
        self.api.scheme() == "http"
    }

    pub(crate) fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.api.as_str().trim_end_matches('/'))
    }

    pub(crate) fn namespace(&self, product: &str) -> String {
        format!(
            "{product}.workos.{}@{}",
            self.client_id,
            self.api.origin().ascii_serialization()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn a_client_id_is_required_and_plain() {
        for value in ["", "client id", "client/../x"] {
            assert!(Config::new(value).is_err(), "{value}");
        }
        let config = Config::new("client_0000000000").unwrap();
        assert_eq!(
            config.endpoint("/user_management/authorize"),
            "https://api.workos.com/user_management/authorize"
        );
        assert_ne!(config.namespace("stable"), config.namespace("preview"));
    }

    #[test]
    fn the_api_origin_is_https_or_loopback_http() {
        let config = Config::new("client_0000000000").unwrap();
        for value in [
            "http://api.example.com",
            "https://api.example.com/path",
            "https://user@api.example.com",
        ] {
            assert!(config.clone().with_api_url(value).is_err(), "{value}");
        }
        assert!(config.clone().with_api_url("http://127.0.0.1:9000").is_ok());
        let default_namespace = config.namespace("fixture");
        let config = config.with_api_url("https://auth.example.com/").unwrap();
        assert_eq!(
            config.endpoint("/user_management/authenticate"),
            "https://auth.example.com/user_management/authenticate"
        );
        assert_ne!(config.namespace("fixture"), default_namespace);
    }

    #[test]
    fn device_authorisation_is_opt_in() {
        let config = Config::new("client_0000000000").unwrap();
        assert_eq!(config.flow, super::Flow::Browser);
        assert_eq!(config.with_device_authorization().flow, super::Flow::Device);
    }

    #[test]
    fn resources_are_registered_once() {
        let config = Config::new("client_0000000000")
            .unwrap()
            .with_resource("example-api")
            .unwrap();
        assert!(config.check_resource("example-api").is_ok());
        assert!(config.check_resource("other").is_err());
        assert!(config.with_resource("example-api").is_err());
    }
}
