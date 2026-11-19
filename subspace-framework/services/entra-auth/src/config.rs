use crate::Error;
use std::collections::BTreeMap;

/// The global Microsoft identity platform. Sovereign clouds and external
/// tenants use [`Config::with_authority`].
pub const DEFAULT_AUTHORITY: &str = "https://login.microsoftonline.com";

const SCOPE_LIMIT: usize = 256;

/// Non-secret, tenant-bound deployment configuration for a dedicated public client.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) tenant: String,
    pub(crate) client: String,
    authority: url::Url,
    resources: BTreeMap<String, String>,
    organization_name: String,
}

impl Config {
    pub fn new(tenant: &str, client: &str) -> Result<Self, Error> {
        let tenant = uuid::Uuid::parse_str(tenant).map_err(|_| Error::Unconfigured)?;
        let client = uuid::Uuid::parse_str(client).map_err(|_| Error::Unconfigured)?;
        if tenant.is_nil() || client.is_nil() {
            return Err(Error::Unconfigured);
        }
        Ok(Self {
            tenant: tenant.to_string(),
            client: client.to_string(),
            authority: url::Url::parse(DEFAULT_AUTHORITY).map_err(|_| Error::Unconfigured)?,
            resources: BTreeMap::new(),
            organization_name: String::new(),
        })
    }

    /// Use another authority host, such as a sovereign cloud's. It must be
    /// an HTTPS origin; discovery may only direct traffic to that host.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_authority(mut self, authority: &str) -> Result<Self, Error> {
        let url = url::Url::parse(authority.trim()).map_err(|_| Error::Unconfigured)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Unconfigured);
        }
        self.authority = url;
        Ok(self)
    }

    /// Label the tenant in the app's sign-in settings.
    // qual:api — Consumed by app crates through the Framework facade.
    pub fn with_organization(mut self, name: &str) -> Self {
        self.organization_name = name.into();
        self
    }

    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }

    /// Public tenant identifier shown in the app's sign-in settings.
    pub fn tenant_id(&self) -> &str {
        &self.tenant
    }

    /// Register an app-owned delegated API by its application ID and
    /// permission, as `api://<client>/<permission>`. Callers cannot request
    /// arbitrary audiences through the session manager.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_resource(self, name: &str, client: &str, permission: &str) -> Result<Self, Error> {
        let client = uuid::Uuid::parse_str(client).map_err(|_| Error::Unconfigured)?;
        if client.is_nil() || !identifier(permission) {
            return Err(Error::Unconfigured);
        }
        self.with_scope(name, &format!("api://{client}/{permission}"))
    }

    /// Register an app-owned delegated API by its full scope, such as
    /// `api://00000000-0000-0000-0000-000000000000/API.Access`.
    pub fn with_scope(mut self, name: &str, scope: &str) -> Result<Self, Error> {
        if !identifier(name) || !valid_scope(scope) || self.resources.contains_key(name) {
            return Err(Error::Unconfigured);
        }
        self.resources.insert(name.into(), scope.into());
        Ok(self)
    }

    pub(crate) fn scope(&self, name: &str) -> Result<&str, Error> {
        self.resources
            .get(name)
            .map(String::as_str)
            .ok_or(Error::Unconfigured)
    }

    pub(crate) fn authority_host(&self) -> &str {
        self.authority.host_str().unwrap_or_default()
    }

    pub(crate) fn issuer(&self) -> String {
        format!(
            "{}/{}/v2.0",
            self.authority.as_str().trim_end_matches('/'),
            self.tenant
        )
    }

    pub(crate) fn namespace(&self, product: &str) -> String {
        format!("{product}.entra.{}.{}", self.tenant, self.client)
    }
}

const IDENTIFIER_LIMIT: usize = 64;

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= IDENTIFIER_LIMIT
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn valid_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= SCOPE_LIMIT
        && (value.starts_with("api://") || value.starts_with("https://"))
        && value.bytes().all(|c| c.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::Config;

    fn config() -> Config {
        Config::new(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        )
        .unwrap()
    }

    #[test]
    fn resources_require_explicit_valid_and_unique_registrations() {
        let config = config();
        let resource = "00000000-0000-0000-0000-000000000004";
        assert!(
            config
                .clone()
                .with_resource("example-api", "common", "API.Access")
                .is_err()
        );
        assert!(
            config
                .clone()
                .with_resource("example-api", resource, "API.Access openid")
                .is_err()
        );
        let config = config
            .with_resource("example-api", resource, "API.Access")
            .unwrap();
        assert_eq!(
            config.scope("example-api").unwrap(),
            "api://00000000-0000-0000-0000-000000000004/API.Access"
        );
        assert!(config.scope("unknown").is_err());
        assert!(
            config
                .with_resource("example-api", resource, "API.Access")
                .is_err()
        );
    }

    #[test]
    fn full_scopes_must_name_an_api_without_spaces() {
        assert!(config().with_scope("api", "openid").is_err());
        assert!(
            config()
                .with_scope("api", "api://x/API.Access openid")
                .is_err()
        );
        let config = config()
            .with_scope(
                "api",
                "api://00000000-0000-0000-0000-000000000004/API.Access",
            )
            .unwrap();
        assert!(config.scope("api").is_ok());
    }

    #[test]
    fn a_tenant_and_dedicated_client_are_required() {
        assert!(Config::new("common", "client").is_err());
        assert!(
            Config::new(
                "00000000-0000-0000-0000-000000000000",
                "00000000-0000-0000-0000-000000000001"
            )
            .is_err()
        );
        let config = config();
        assert_eq!(
            config.issuer(),
            "https://login.microsoftonline.com/00000000-0000-0000-0000-000000000001/v2.0"
        );
        assert_ne!(config.namespace("stable"), config.namespace("preview"));
    }

    #[test]
    fn the_authority_is_an_https_origin() {
        for value in [
            "http://login.example.com",
            "https://login.example.com/tenant",
            "https://user@login.example.com",
            "not a url",
        ] {
            assert!(config().with_authority(value).is_err(), "{value}");
        }
        let config = config()
            .with_authority("https://login.example.com/")
            .unwrap();
        assert_eq!(config.authority_host(), "login.example.com");
        assert_eq!(
            config.issuer(),
            "https://login.example.com/00000000-0000-0000-0000-000000000001/v2.0"
        );
    }
}
