//! The app's identity provider and Subspace API origin. The selected build
//! config (`build-config.json`, or `SUBSPACE_BUILD_CONFIG`) sets the defaults;
//! the same-named `SUBSPACE_*` environment variables override them at run
//! time. Nothing here names a real tenant, client or API.

use std::sync::Arc;
use std::time::Duration;
use subspace_framework::account::{Error, Provider};
use subspace_framework::shell::{AccountRegistration, AccountTokenSource};

const MINIMUM_API_TOKEN_VALIDITY: Duration = Duration::from_secs(120);
/// The resource name the app registers for Subspace API tokens.
const API_RESOURCE: &str = "subspace-api";

/// One setting: its run-time variable and its build-time value.
struct Setting {
    env: &'static str,
    built: &'static str,
}

impl Setting {
    fn value(&self) -> String {
        resolve(std::env::var(self.env).ok(), self.built)
    }
}

const PROVIDER: Setting = Setting {
    env: "SUBSPACE_IDENTITY_PROVIDER",
    built: env!("SUBSPACE_BUILD_IDENTITY_PROVIDER"),
};
const API_URL: Setting = Setting {
    env: "SUBSPACE_API_URL",
    built: env!("SUBSPACE_BUILD_API_URL"),
};
const WORKOS_CLIENT_ID: Setting = Setting {
    env: "SUBSPACE_WORKOS_CLIENT_ID",
    built: env!("SUBSPACE_BUILD_WORKOS_CLIENT_ID"),
};
const WORKOS_API_URL: Setting = Setting {
    env: "SUBSPACE_WORKOS_API_URL",
    built: env!("SUBSPACE_BUILD_WORKOS_API_URL"),
};
const WORKOS_REDIRECT_PORT: Setting = Setting {
    env: "SUBSPACE_WORKOS_REDIRECT_PORT",
    built: env!("SUBSPACE_BUILD_WORKOS_REDIRECT_PORT"),
};
const ENTRA_TENANT_ID: Setting = Setting {
    env: "SUBSPACE_ENTRA_TENANT_ID",
    built: env!("SUBSPACE_BUILD_ENTRA_TENANT_ID"),
};
const ENTRA_CLIENT_ID: Setting = Setting {
    env: "SUBSPACE_ENTRA_CLIENT_ID",
    built: env!("SUBSPACE_BUILD_ENTRA_CLIENT_ID"),
};
const ENTRA_AUTHORITY: Setting = Setting {
    env: "SUBSPACE_ENTRA_AUTHORITY",
    built: env!("SUBSPACE_BUILD_ENTRA_AUTHORITY"),
};
const ENTRA_API_SCOPE: Setting = Setting {
    env: "SUBSPACE_ENTRA_API_SCOPE",
    built: env!("SUBSPACE_BUILD_ENTRA_API_SCOPE"),
};
const DEV_EMAIL: Setting = Setting {
    env: "SUBSPACE_DEV_EMAIL",
    built: env!("SUBSPACE_BUILD_DEV_EMAIL"),
};
const DEV_NAME: Setting = Setting {
    env: "SUBSPACE_DEV_NAME",
    built: env!("SUBSPACE_BUILD_DEV_NAME"),
};
const DEV_ORGANISATION: Setting = Setting {
    env: "SUBSPACE_DEV_ORGANISATION",
    built: env!("SUBSPACE_BUILD_DEV_ORGANISATION"),
};
const ORGANIZATION_NAME: &str = env!("SUBSPACE_BUILD_ORGANIZATION_NAME");

/// A set variable wins, even when empty: `SUBSPACE_IDENTITY_PROVIDER=` turns
/// sign-in off for one run.
fn resolve(runtime: Option<String>, built: &str) -> String {
    runtime
        .unwrap_or_else(|| built.to_owned())
        .trim()
        .to_owned()
}

/// Installs the API base URL before Spaces start. Empty leaves Issues and
/// the Channels initiative picker without a service.
pub(crate) fn configure_api_origin() -> Result<(), subspace_api_client::Error> {
    let url = API_URL.value();
    if url.is_empty() {
        return Ok(());
    }
    subspace_api_client::configure_api_origin(&url)
}

/// The native account the shell offers, when the config names a provider.
pub(crate) fn registration() -> Option<AccountRegistration> {
    (!PROVIDER.value().is_empty()).then_some(AccountRegistration {
        provider,
        configure: Some(configure_subspace_api),
    })
}

/// `namespace` is the bundle identifier, which keeps keychain entries apart.
fn provider(namespace: &str) -> Option<Arc<dyn Provider>> {
    let built: Result<Arc<dyn Provider>, Error> = match PROVIDER.value().as_str() {
        "dev" => dev(),
        "workos" => workos(namespace),
        "entra" => entra(namespace),
        _ => Err(Error::Unconfigured),
    };
    match built {
        Ok(provider) => Some(provider),
        Err(error) => {
            eprintln!(
                "Sign-in is off: the identity provider configuration is incomplete ({error})"
            );
            None
        }
    }
}

fn dev() -> Result<Arc<dyn Provider>, Error> {
    let user = subspace_api_client::DevUser {
        email: DEV_EMAIL.value(),
        name: DEV_NAME.value(),
        organisation: DEV_ORGANISATION.value(),
    };
    Ok(Arc::new(subspace_api_client::DevSignIn::new(
        &API_URL.value(),
        user,
    )?))
}

fn workos(namespace: &str) -> Result<Arc<dyn Provider>, Error> {
    use subspace_framework::workos_auth::{Config, SessionManager};
    let mut config = Config::new(&WORKOS_CLIENT_ID.value())?
        .with_organization(ORGANIZATION_NAME)
        .with_resource(API_RESOURCE)?;
    let api = WORKOS_API_URL.value();
    if !api.is_empty() {
        config = config.with_api_url(&api)?;
    }
    let port = WORKOS_REDIRECT_PORT.value();
    if !port.is_empty() {
        config = config.with_redirect_port(port.parse().map_err(|_| Error::Unconfigured)?);
    }
    Ok(Arc::new(SessionManager::system(config, namespace)))
}

fn entra(namespace: &str) -> Result<Arc<dyn Provider>, Error> {
    use subspace_framework::entra_auth::{Config, SessionManager};
    let mut config = Config::new(&ENTRA_TENANT_ID.value(), &ENTRA_CLIENT_ID.value())?
        .with_organization(ORGANIZATION_NAME);
    let authority = ENTRA_AUTHORITY.value();
    if !authority.is_empty() {
        config = config.with_authority(&authority)?;
    }
    let scope = ENTRA_API_SCOPE.value();
    if !scope.is_empty() {
        config = config.with_scope(API_RESOURCE, &scope)?;
    }
    Ok(Arc::new(SessionManager::system(config, namespace)))
}

fn configure_subspace_api(source: AccountTokenSource) {
    let account = source.clone();
    subspace_api_client::use_native_account(
        move || account.generation(),
        move || {
            source
                .token_for(API_RESOURCE, MINIMUM_API_TOKEN_VALIDITY)
                .map(|grant| grant.token)
                .map_err(api_error)
        },
    );
}

fn api_error(error: Error) -> subspace_api_client::Error {
    match error {
        Error::Offline | Error::Service => subspace_api_client::Error::Network,
        _ => subspace_api_client::Error::SignInNeeded,
    }
}

#[cfg(test)]
mod tests {
    use super::{api_error, resolve};
    use subspace_framework::account::Error;

    #[test]
    fn a_run_time_variable_overrides_the_build_config_even_when_empty() {
        assert_eq!(resolve(None, " dev "), "dev");
        assert_eq!(resolve(Some("workos".into()), "dev"), "workos");
        assert_eq!(resolve(Some(String::new()), "dev"), "");
    }

    #[test]
    fn provider_failures_reach_the_api_client_as_network_or_sign_in() {
        assert_eq!(
            api_error(Error::Offline),
            subspace_api_client::Error::Network
        );
        assert_eq!(
            api_error(Error::SignInRequired),
            subspace_api_client::Error::SignInNeeded
        );
    }
}
