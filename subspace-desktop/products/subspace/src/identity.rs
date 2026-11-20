//! The app's tracker backend, identity provider and API origin. The selected
//! build config (`build-config.json`, or `SUBSPACE_BUILD_CONFIG`) sets the
//! defaults; the same-named `SUBSPACE_*` environment variables override them
//! at run time. A NETT build (`api_backend: "nett"`) signs in with WorkOS
//! device authorisation, talks only to its NETT API and refuses every
//! Subspace API setting. Nothing here names a real tenant, client or API.

use crate::launch::{self, Launch, Stamped};
use crate::settings::ApiBackend;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use subspace_framework::account::{Error, Provider};
use subspace_framework::shell::{AccountRegistration, AccountTokenSource, account_session};

const MINIMUM_API_TOKEN_VALIDITY: Duration = Duration::from_secs(120);
/// The resource name the app registers for Subspace API tokens.
const API_RESOURCE: &str = "subspace-api";
/// The resource name a NETT build registers for NETT API tokens.
const NETT_RESOURCE: &str = "nett-api";
const ORGANIZATION_NAME: &str = env!("SUBSPACE_BUILD_ORGANIZATION_NAME");

/// What the build script stamped from the selected build config.
const STAMPED: Stamped = Stamped {
    backend: env!("SUBSPACE_BUILD_API_BACKEND"),
    provider: env!("SUBSPACE_BUILD_IDENTITY_PROVIDER"),
    api_url: env!("SUBSPACE_BUILD_API_URL"),
    nett_api_url: env!("SUBSPACE_BUILD_NETT_API_URL"),
    workos_client_id: env!("SUBSPACE_BUILD_WORKOS_CLIENT_ID"),
    workos_api_url: env!("SUBSPACE_BUILD_WORKOS_API_URL"),
    workos_redirect_port: env!("SUBSPACE_BUILD_WORKOS_REDIRECT_PORT"),
    entra_tenant_id: env!("SUBSPACE_BUILD_ENTRA_TENANT_ID"),
    entra_client_id: env!("SUBSPACE_BUILD_ENTRA_CLIENT_ID"),
    entra_authority: env!("SUBSPACE_BUILD_ENTRA_AUTHORITY"),
    entra_api_scope: env!("SUBSPACE_BUILD_ENTRA_API_SCOPE"),
    dev_email: env!("SUBSPACE_BUILD_DEV_EMAIL"),
    dev_name: env!("SUBSPACE_BUILD_DEV_NAME"),
    dev_organisation: env!("SUBSPACE_BUILD_DEV_ORGANISATION"),
};

/// The run's configuration, resolved once at launch; every later reader
/// sees the same values.
static LAUNCH: OnceLock<Launch> = OnceLock::new();

/// Whether this is a NETT build.
pub(crate) fn nett() -> bool {
    ApiBackend::parse(STAMPED.backend) == Some(ApiBackend::Nett)
}

/// The run's `SUBSPACE_*` variables. A value that is not Unicode counts as
/// unset, as `std::env::var` treats it.
fn variables() -> impl Iterator<Item = (String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
}

/// Resolves the launch from the stamped settings and the environment, then
/// installs the build's one backend before Spaces start. A wrong setting
/// installs nothing, so a NETT build never reaches Subspace API.
pub(crate) fn configure_backend() -> Result<(), String> {
    let resolved = launch::resolve(&STAMPED, variables())?;
    launch::install(&resolved)?;
    let _ = LAUNCH.set(resolved);
    Ok(())
}

/// The native account the shell offers, when the config names a provider.
pub(crate) fn registration() -> Option<AccountRegistration> {
    let launch = LAUNCH.get()?;
    if launch.provider.is_empty() {
        return None;
    }
    Some(if launch.backend == ApiBackend::Nett {
        AccountRegistration {
            provider: nett_provider,
            configure: Some(configure_nett_api),
        }
    } else {
        AccountRegistration {
            provider,
            configure: Some(configure_subspace_api),
        }
    })
}

/// WorkOS device authorisation with the environment's one accepted client.
fn nett_provider(namespace: &str) -> Option<Arc<dyn Provider>> {
    use subspace_framework::workos_auth::{Config, SessionManager};
    let built = (|| -> Result<Arc<dyn Provider>, Error> {
        let launch = LAUNCH.get().ok_or(Error::Unconfigured)?;
        let mut config = Config::new(&launch.workos_client_id)?
            .with_organization(ORGANIZATION_NAME)
            .with_resource(NETT_RESOURCE)?
            .with_device_authorization();
        if !launch.workos_api_url.is_empty() {
            config = config.with_api_url(&launch.workos_api_url)?;
        }
        Ok(Arc::new(SessionManager::system(config, namespace)))
    })();
    match built {
        Ok(provider) => Some(provider),
        Err(error) => {
            eprintln!("Sign-in is off: the WorkOS configuration is incomplete ({error})");
            None
        }
    }
}

/// `namespace` is the bundle identifier, which keeps keychain entries apart.
fn provider(namespace: &str) -> Option<Arc<dyn Provider>> {
    let built: Result<Arc<dyn Provider>, Error> = match LAUNCH.get() {
        Some(launch) => match launch.provider.as_str() {
            "dev" => dev(launch),
            "workos" => workos(launch, namespace),
            "entra" => entra(launch, namespace),
            _ => Err(Error::Unconfigured),
        },
        None => Err(Error::Unconfigured),
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

fn dev(launch: &Launch) -> Result<Arc<dyn Provider>, Error> {
    let user = subspace_api_client::DevUser {
        email: launch.dev_email.clone(),
        name: launch.dev_name.clone(),
        organisation: launch.dev_organisation.clone(),
    };
    Ok(Arc::new(subspace_api_client::DevSignIn::new(
        &launch.api_url,
        user,
    )?))
}

fn workos(launch: &Launch, namespace: &str) -> Result<Arc<dyn Provider>, Error> {
    use subspace_framework::workos_auth::{Config, SessionManager};
    let mut config = Config::new(&launch.workos_client_id)?
        .with_organization(ORGANIZATION_NAME)
        .with_resource(API_RESOURCE)?;
    if !launch.workos_api_url.is_empty() {
        config = config.with_api_url(&launch.workos_api_url)?;
    }
    if !launch.workos_redirect_port.is_empty() {
        config = config.with_redirect_port(
            launch
                .workos_redirect_port
                .parse()
                .map_err(|_| Error::Unconfigured)?,
        );
    }
    Ok(Arc::new(SessionManager::system(config, namespace)))
}

fn entra(launch: &Launch, namespace: &str) -> Result<Arc<dyn Provider>, Error> {
    use subspace_framework::entra_auth::{Config, SessionManager};
    let mut config = Config::new(&launch.entra_tenant_id, &launch.entra_client_id)?
        .with_organization(ORGANIZATION_NAME);
    if !launch.entra_authority.is_empty() {
        config = config.with_authority(&launch.entra_authority)?;
    }
    if !launch.entra_api_scope.is_empty() {
        config = config.with_scope(API_RESOURCE, &launch.entra_api_scope)?;
    }
    Ok(Arc::new(SessionManager::system(config, namespace)))
}

fn configure_subspace_api(source: AccountTokenSource) {
    use_account(source, API_RESOURCE);
}

fn configure_nett_api(source: AccountTokenSource) {
    use_account(source, NETT_RESOURCE);
}

/// Long enough that any held token counts as too short-lived, so asking
/// for it renews the token.
const FORCE_RENEWAL: std::time::Duration = std::time::Duration::from_secs(10 * 365 * 24 * 3600);

fn use_account(source: AccountTokenSource, resource: &'static str) {
    let account = source.clone();
    let renewing = source.clone();
    // After the service refuses a token the app still holds, ask for one so
    // long-lived that the held token cannot satisfy it: the provider renews
    // and keeps the fresh token, which the next ordinary request returns.
    subspace_api_client::use_token_renewal(move || {
        let _ = renewing.token_for(resource, FORCE_RENEWAL);
        renewing
            .token_for(resource, MINIMUM_API_TOKEN_VALIDITY)
            .is_ok()
    });
    subspace_api_client::use_native_account(
        move || account.generation(),
        |cx| {
            account_session(cx)
                .filter(|session| session.is_current())
                .map(|session| session.generation())
        },
        move || {
            source
                .token_for(resource, MINIMUM_API_TOKEN_VALIDITY)
                .map(|grant| grant.token)
                .map_err(api_error)
        },
    );
}

/// A NETT build has no update channel of its own yet, so the public
/// updater must not replace it, and Issues is its main Space.
pub(crate) fn adjust_descriptor(descriptor: &mut subspace_framework::product::ProductDescriptor) {
    if !nett() {
        return;
    }
    descriptor.update = None;
    for board in &mut descriptor.boards {
        if board.id == "issues" {
            board.default_enabled = true;
        }
    }
}

fn api_error(error: Error) -> subspace_api_client::Error {
    match error {
        Error::Offline | Error::Service => subspace_api_client::Error::Network,
        _ => subspace_api_client::Error::SignInNeeded,
    }
}

#[cfg(test)]
mod tests {
    use super::{STAMPED, api_error};
    use crate::launch::resolve;
    use subspace_framework::account::Error;

    #[test]
    fn the_stamped_build_resolves_without_run_time_overrides() {
        // Whatever config this binary was stamped with, it must resolve on
        // its own: the build script already refused an invalid one.
        let launch = resolve(&STAMPED, Vec::new()).unwrap();
        assert_eq!(
            launch.backend == crate::settings::ApiBackend::Nett,
            super::nett()
        );
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
