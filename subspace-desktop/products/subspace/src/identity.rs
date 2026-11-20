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
pub(crate) fn variables() -> impl Iterator<Item = (String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
}

/// Resolves the launch from the stamped settings and the run's `variables`
/// (see [`variables`]), then installs the build's one backend before Spaces
/// start. A wrong setting installs nothing, so a NETT build never reaches
/// Subspace API.
pub(crate) fn configure_backend(
    variables: impl IntoIterator<Item = (String, String)>,
) -> Result<(), String> {
    let resolved = launch::resolve(&STAMPED, variables)?;
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
        let credentials = nett_credentials(namespace, &launch.nett_api_url);
        Ok(Arc::new(SessionManager::system(config, &credentials)))
    })();
    match built {
        Ok(provider) => Some(provider),
        Err(error) => {
            eprintln!("Sign-in is off: the WorkOS configuration is incomplete ({error})");
            None
        }
    }
}

/// Where a NETT build keeps its WorkOS sign-in: apart for each NETT API
/// origin. A sign-in kept for one origin is never restored, and so its
/// bearer is never sent, when a launch points the build at another, such as
/// a loopback fixture: that origin asks for its own sign-in first.
fn nett_credentials(namespace: &str, nett_api_url: &str) -> String {
    let origin = url::Url::parse(nett_api_url).map_or_else(
        |_| nett_api_url.to_owned(),
        |url| url.origin().ascii_serialization(),
    );
    format!("{namespace}.nett@{origin}")
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
    Ok(Arc::new(subspace_api_client::DevSignIn::new(
        &launch.api_url,
        launch.dev_user.clone(),
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
        renewed(|minimum| {
            renewing
                .token_for(resource, minimum)
                .ok()
                .map(|grant| fingerprint(grant.token.expose()))
        })
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

/// Whether a forced renewal left the provider holding a different token.
/// A refresh that failed leaves the refused token in place, still valid by
/// its own clock: that is no renewal, so the caller treats the refusal as
/// standing rather than trying the same bearer again as if it were fresh.
fn renewed(token_for: impl Fn(Duration) -> Option<u64>) -> bool {
    let refused = token_for(MINIMUM_API_TOKEN_VALIDITY);
    let _ = token_for(FORCE_RENEWAL);
    let after = token_for(MINIMUM_API_TOKEN_VALIDITY);
    after.is_some() && after != refused
}

/// Tells tokens apart without keeping a copy of either.
fn fingerprint(token: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

/// Adjusts the app's descriptor for a NETT build; see [`adjust_for_nett`].
pub(crate) fn adjust_descriptor(descriptor: &mut subspace_framework::product::ProductDescriptor) {
    if nett() {
        adjust_for_nett(descriptor);
    }
}

/// A NETT build has no update channel of its own yet, so the public
/// updater must not replace it, and Issues is its main Space.
fn adjust_for_nett(descriptor: &mut subspace_framework::product::ProductDescriptor) {
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
    use super::{
        FORCE_RENEWAL, LAUNCH, STAMPED, adjust_descriptor, adjust_for_nett, api_error,
        configure_backend, nett_credentials, renewed,
    };
    use crate::launch::{API_URL, NETT_API_URL, resolve};
    use std::cell::Cell;
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

    /// Set for the child process that installs the stamped launch.
    const INSTALL_CHILD: &str = "SUBSPACE_IDENTITY_TEST_INSTALL";

    #[test]
    fn the_stamped_build_installs_its_one_backend_and_a_refused_setting_nothing() {
        // A setting the build refuses installs nothing.
        let (name, value) = if super::nett() {
            (API_URL, "http://127.0.0.1:3000")
        } else {
            (NETT_API_URL, "https://api.nett.example")
        };
        assert!(configure_backend([(name.to_owned(), value.to_owned())]).is_err());
        assert!(LAUNCH.get().is_none());
        // Without overrides the stamped launch is installed, as at startup.
        // Installing sets the process's one backend and origin and, in a
        // NETT build, turns on organisation memory on disk, so it happens in
        // a child process with a home of its own, never in this one.
        let home = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        for name in [
            "HOME",
            "USERPROFILE",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            child.env(name, home.path());
        }
        let run = child
            .args([
                "--ignored",
                "--exact",
                "identity::tests::installs_the_stamped_launch_in_a_child",
            ])
            .env(INSTALL_CHILD, "1")
            .output()
            .unwrap();
        assert!(
            run.status.success() && String::from_utf8_lossy(&run.stdout).contains("1 passed"),
            "{} {}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(LAUNCH.get().is_none(), "nothing was installed here");
    }

    #[test]
    #[ignore = "a child process of the_stamped_build_installs_its_one_backend_and_a_refused_setting_nothing"]
    fn installs_the_stamped_launch_in_a_child() {
        if std::env::var_os(INSTALL_CHILD).is_none() {
            return;
        }
        configure_backend(Vec::new()).unwrap();
        assert_eq!(LAUNCH.get(), Some(&resolve(&STAMPED, Vec::new()).unwrap()));
        assert_eq!(
            subspace_api_client::backend() == subspace_api_client::Backend::Nett,
            super::nett()
        );
    }

    #[test]
    fn a_nett_build_has_no_public_updater_and_enables_issues_by_default() {
        let subspace = subspace_descriptor::subspace_descriptor();
        let mut nett = subspace.clone();
        adjust_for_nett(&mut nett);
        assert!(nett.update.is_none());
        assert!(
            nett.boards
                .iter()
                .any(|board| board.id == "issues" && board.default_enabled)
        );
        // Every other Space keeps its own default.
        assert_eq!(nett.boards.len(), subspace.boards.len());
        for (after, before) in nett.boards.iter().zip(&subspace.boards) {
            assert!(after.id == "issues" || after == before);
        }
        // The stamped build is adjusted exactly when it is a NETT build.
        let mut stamped = subspace.clone();
        adjust_descriptor(&mut stamped);
        assert_eq!(stamped, if super::nett() { nett } else { subspace });
    }

    #[test]
    fn a_nett_sign_in_is_kept_apart_for_each_api_origin() {
        let built = nett_credentials("app.example", "https://api.nett.example/api");
        assert_eq!(
            built,
            nett_credentials("app.example", "https://api.nett.example")
        );
        // A launch pointing elsewhere, a fixture included, never restores
        // the sign-in kept for the build's own NETT API.
        for elsewhere in [
            "http://127.0.0.1:8080/api",
            "https://other.nett.example/api",
            "https://api.nett.example:8443/api",
        ] {
            assert_ne!(built, nett_credentials("app.example", elsewhere));
        }
    }

    #[test]
    fn a_renewal_that_kept_the_refused_token_is_no_renewal() {
        // The refresh fails: the provider still hands out the refused token.
        let held = Cell::new(1_u64);
        assert!(!renewed(|_| Some(held.get())));
        // The refresh succeeds: the next ordinary request gets a new token.
        assert!(renewed(|minimum| {
            if minimum == FORCE_RENEWAL {
                held.set(2);
                None
            } else {
                Some(held.get())
            }
        }));
        // No token at all afterwards is no renewal either.
        assert!(!renewed(|_| None));
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
