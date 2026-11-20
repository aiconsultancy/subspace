//! Launch resolution: the settings the build stamped and the run's
//! `SUBSPACE_*` variables become one checked configuration before anything
//! is installed. It is a pure function of both, so tests can resolve either
//! build variant whatever the test binary itself was stamped with.
//!
//! A set variable wins over the stamped value, even when empty:
//! `SUBSPACE_IDENTITY_PROVIDER=` turns sign-in off for one run. A Subspace
//! API build refuses NETT settings, and a NETT build refuses every Subspace
//! API setting, signs in only with WorkOS and needs a valid NETT API URL.

use crate::settings::{ApiBackend, refused_in_nett, valid_api_url, valid_workos_client_id};
use std::collections::BTreeMap;

pub(crate) const PROVIDER: &str = "SUBSPACE_IDENTITY_PROVIDER";
pub(crate) const API_URL: &str = "SUBSPACE_API_URL";
pub(crate) const NETT_API_URL: &str = "SUBSPACE_NETT_API_URL";
pub(crate) const WORKOS_CLIENT_ID: &str = "SUBSPACE_WORKOS_CLIENT_ID";
pub(crate) const WORKOS_API_URL: &str = "SUBSPACE_WORKOS_API_URL";
pub(crate) const WORKOS_REDIRECT_PORT: &str = "SUBSPACE_WORKOS_REDIRECT_PORT";
pub(crate) const ENTRA_TENANT_ID: &str = "SUBSPACE_ENTRA_TENANT_ID";
pub(crate) const ENTRA_CLIENT_ID: &str = "SUBSPACE_ENTRA_CLIENT_ID";
pub(crate) const ENTRA_AUTHORITY: &str = "SUBSPACE_ENTRA_AUTHORITY";
pub(crate) const ENTRA_API_SCOPE: &str = "SUBSPACE_ENTRA_API_SCOPE";
pub(crate) const DEV_EMAIL: &str = "SUBSPACE_DEV_EMAIL";
pub(crate) const DEV_NAME: &str = "SUBSPACE_DEV_NAME";
pub(crate) const DEV_ORGANISATION: &str = "SUBSPACE_DEV_ORGANISATION";

/// The values the build script stamped from the build config.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Stamped {
    pub backend: &'static str,
    pub provider: &'static str,
    pub api_url: &'static str,
    pub nett_api_url: &'static str,
    pub workos_client_id: &'static str,
    pub workos_api_url: &'static str,
    pub workos_redirect_port: &'static str,
    pub entra_tenant_id: &'static str,
    pub entra_client_id: &'static str,
    pub entra_authority: &'static str,
    pub entra_api_scope: &'static str,
    pub dev_email: &'static str,
    pub dev_name: &'static str,
    pub dev_organisation: &'static str,
}

/// The run's one resolved configuration. A NETT launch carries no Subspace
/// API URL, and a Subspace launch no NETT one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Launch {
    pub backend: ApiBackend,
    /// `""` (sign-in off), `dev`, `workos` or `entra`.
    pub provider: String,
    pub api_url: String,
    pub nett_api_url: String,
    pub workos_client_id: String,
    pub workos_api_url: String,
    pub workos_redirect_port: String,
    pub entra_tenant_id: String,
    pub entra_client_id: String,
    pub entra_authority: String,
    pub entra_api_scope: String,
    pub dev_email: String,
    pub dev_name: String,
    pub dev_organisation: String,
}

/// Resolves the launch from the stamped settings and the run's variables,
/// such as `std::env::vars()`. Nothing is installed.
pub(crate) fn resolve(
    stamped: &Stamped,
    variables: impl IntoIterator<Item = (String, String)>,
) -> Result<Launch, String> {
    let variables: BTreeMap<String, String> = variables
        .into_iter()
        .filter(|(name, _)| name.starts_with("SUBSPACE_"))
        .collect();
    let value = |name: &str, built: &str| {
        variables
            .get(name)
            .map_or(built, String::as_str)
            .trim()
            .to_owned()
    };
    let backend = ApiBackend::parse(stamped.backend)
        .ok_or_else(|| "The build's api_backend must be subspace or nett.".to_owned())?;
    let mut launch = Launch {
        backend,
        provider: value(PROVIDER, stamped.provider),
        api_url: value(API_URL, stamped.api_url),
        nett_api_url: value(NETT_API_URL, stamped.nett_api_url),
        workos_client_id: value(WORKOS_CLIENT_ID, stamped.workos_client_id),
        workos_api_url: value(WORKOS_API_URL, stamped.workos_api_url),
        workos_redirect_port: value(WORKOS_REDIRECT_PORT, stamped.workos_redirect_port),
        entra_tenant_id: value(ENTRA_TENANT_ID, stamped.entra_tenant_id),
        entra_client_id: value(ENTRA_CLIENT_ID, stamped.entra_client_id),
        entra_authority: value(ENTRA_AUTHORITY, stamped.entra_authority),
        entra_api_scope: value(ENTRA_API_SCOPE, stamped.entra_api_scope),
        dev_email: value(DEV_EMAIL, stamped.dev_email),
        dev_name: value(DEV_NAME, stamped.dev_name),
        dev_organisation: value(DEV_ORGANISATION, stamped.dev_organisation),
    };
    match backend {
        ApiBackend::Subspace => {
            if variables
                .get(NETT_API_URL)
                .is_some_and(|url| !url.trim().is_empty())
            {
                return Err(format!(
                    "This Subspace API build does not use NETT settings. Unset {NETT_API_URL} and start again."
                ));
            }
            if !launch.api_url.is_empty() && !valid_api_url(&launch.api_url) {
                return Err(subspace_url_error());
            }
            launch.nett_api_url.clear();
        }
        ApiBackend::Nett => {
            check_nett(&launch, &variables)?;
            launch.api_url.clear();
        }
    }
    Ok(launch)
}

/// A NETT build's rules: no Subspace-only variable, the WorkOS provider or
/// none, a valid client ID whenever sign-in is on or the run overrides it,
/// and a valid NETT API URL.
fn check_nett(launch: &Launch, variables: &BTreeMap<String, String>) -> Result<(), String> {
    let refused = refused_in_nett(
        variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    if !refused.is_empty() {
        return Err(format!(
            "This NETT build does not use Subspace API settings. Unset {} and start again.",
            refused.join(", ")
        ));
    }
    if !matches!(launch.provider.as_str(), "workos" | "") {
        return Err(format!(
            "A NETT build signs in with WorkOS. Set {PROVIDER} to workos or leave it unset."
        ));
    }
    let checked = !launch.provider.is_empty() || variables.contains_key(WORKOS_CLIENT_ID);
    if checked && !valid_workos_client_id(&launch.workos_client_id) {
        return Err(format!(
            "A NETT build requires a WorkOS client ID (workos_client_id or {WORKOS_CLIENT_ID}), such as client_01ABC."
        ));
    }
    if !valid_api_url(&launch.nett_api_url) {
        return Err(nett_url_error());
    }
    Ok(())
}

pub(crate) fn nett_url_error() -> String {
    format!(
        "The NETT API URL (nett_api_url or {NETT_API_URL}) must be HTTPS (or HTTP on a loopback address), optionally ending in /api, without credentials, query or fragment."
    )
}

fn subspace_url_error() -> String {
    "The Subspace API URL must be HTTPS (or HTTP on a loopback address), optionally ending in /api, without credentials, query or fragment.".into()
}

/// Installs the launch's one backend before any Space starts. A NETT launch
/// installs only its NETT origin; an empty Subspace API URL leaves Issues and
/// the Channels initiative picker without a service.
pub(crate) fn install(launch: &Launch) -> Result<(), String> {
    match launch.backend {
        ApiBackend::Nett => {
            subspace_api_client::configure_nett_origin(&launch.nett_api_url)
                .map_err(|_| nett_url_error())?;
            subspace_api_client::nett::remember_organisations();
            Ok(())
        }
        ApiBackend::Subspace if launch.api_url.is_empty() => Ok(()),
        ApiBackend::Subspace => subspace_api_client::configure_api_origin(&launch.api_url)
            .map_err(|_| subspace_url_error()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiBackend, Stamped, resolve};

    const CLIENT: &str = "client_00000000000000000000000000";

    /// The checked-in development build: Subspace API with dev sign-in.
    const SUBSPACE: Stamped = Stamped {
        backend: "",
        provider: "dev",
        api_url: "http://127.0.0.1:3000/api",
        nett_api_url: "",
        workos_client_id: "",
        workos_api_url: "",
        workos_redirect_port: "",
        entra_tenant_id: "",
        entra_client_id: "",
        entra_authority: "",
        entra_api_scope: "",
        dev_email: "developer@example.com",
        dev_name: "Developer",
        dev_organisation: "Example",
    };

    /// A NETT build signing in with WorkOS.
    const NETT: Stamped = Stamped {
        backend: "nett",
        provider: "workos",
        api_url: "",
        nett_api_url: "https://api.nett.example",
        workos_client_id: CLIENT,
        workos_api_url: "",
        workos_redirect_port: "",
        entra_tenant_id: "",
        entra_client_id: "",
        entra_authority: "",
        entra_api_scope: "",
        dev_email: "",
        dev_name: "",
        dev_organisation: "",
    };

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_subspace_build_resolves_its_stamped_settings_and_overrides() {
        let launch = resolve(&SUBSPACE, vars(&[("HOME", "/tmp")])).unwrap();
        assert_eq!(launch.backend, ApiBackend::Subspace);
        assert_eq!(launch.provider, "dev");
        assert_eq!(launch.api_url, "http://127.0.0.1:3000/api");
        let launch = resolve(
            &SUBSPACE,
            vars(&[
                ("SUBSPACE_IDENTITY_PROVIDER", ""),
                ("SUBSPACE_API_URL", " https://api.example.com/api "),
                ("SUBSPACE_NETT_API_URL", "  "),
            ]),
        )
        .unwrap();
        assert_eq!(launch.provider, "", "an empty override turns sign-in off");
        assert_eq!(launch.api_url, "https://api.example.com/api");
        assert_eq!(launch.nett_api_url, "");
        let empty = resolve(&SUBSPACE, vars(&[("SUBSPACE_API_URL", "")])).unwrap();
        assert_eq!(empty.api_url, "", "an empty URL leaves Issues unconfigured");
    }

    #[test]
    fn a_subspace_build_refuses_nett_settings_and_invalid_urls() {
        let refused = resolve(
            &SUBSPACE,
            vars(&[("SUBSPACE_NETT_API_URL", "https://api.nett.example")]),
        )
        .unwrap_err();
        assert!(refused.contains("SUBSPACE_NETT_API_URL"), "{refused}");
        let invalid = resolve(
            &SUBSPACE,
            vars(&[("SUBSPACE_API_URL", "http://api.example.com")]),
        )
        .unwrap_err();
        assert!(invalid.contains("Subspace API URL"), "{invalid}");
    }

    #[test]
    fn a_nett_build_refuses_subspace_overrides_before_anything_is_installed() {
        let refused = resolve(
            &NETT,
            vars(&[
                ("SUBSPACE_API_URL", "http://127.0.0.1:3000"),
                ("SUBSPACE_DEV_EMAIL", "developer@example.com"),
                ("SUBSPACE_API_ACCESS_JWT_FILE", ""),
            ]),
        )
        .unwrap_err();
        assert!(
            refused.contains("Unset SUBSPACE_API_URL, SUBSPACE_DEV_EMAIL and"),
            "{refused}"
        );
        // Empty Subspace variables are not settings, and the stamped
        // configuration resolves.
        let launch = resolve(&NETT, vars(&[("SUBSPACE_API_URL", "")])).unwrap();
        assert_eq!(launch.backend, ApiBackend::Nett);
        assert_eq!(launch.nett_api_url, "https://api.nett.example");
        assert_eq!(launch.api_url, "", "a NETT launch has no Subspace API URL");
        assert_eq!(launch.workos_client_id, CLIENT);
    }

    #[test]
    fn a_nett_build_signs_in_only_with_workos_and_a_valid_client() {
        let dev = resolve(&NETT, vars(&[("SUBSPACE_IDENTITY_PROVIDER", "dev")])).unwrap_err();
        assert!(dev.contains("signs in with WorkOS"), "{dev}");
        for client in ["", "client id", "client_"] {
            let error = resolve(&NETT, vars(&[("SUBSPACE_WORKOS_CLIENT_ID", client)])).unwrap_err();
            assert!(error.contains("WorkOS client ID"), "{client:?}: {error}");
        }
        let stamped_without_client = Stamped {
            workos_client_id: "",
            ..NETT
        };
        assert!(resolve(&stamped_without_client, Vec::new()).is_err());
    }

    #[test]
    fn a_nett_build_with_sign_in_off_still_checks_an_overridden_client() {
        let off = |client: Option<&str>| {
            let mut pairs = vec![("SUBSPACE_IDENTITY_PROVIDER", "")];
            pairs.extend(client.map(|client| ("SUBSPACE_WORKOS_CLIENT_ID", client)));
            resolve(
                &Stamped {
                    workos_client_id: "",
                    ..NETT
                },
                vars(&pairs),
            )
        };
        assert_eq!(off(None).unwrap().provider, "");
        assert_eq!(off(Some(CLIENT)).unwrap().workos_client_id, CLIENT);
        assert!(off(Some("not-a-client")).is_err());
        assert!(
            off(Some("")).is_err(),
            "an empty override is not a client ID"
        );
    }

    #[test]
    fn a_nett_build_needs_a_valid_nett_api_url() {
        for url in ["", "http://api.nett.example", "https://api.nett.example/v1"] {
            let error = resolve(&NETT, vars(&[("SUBSPACE_NETT_API_URL", url)])).unwrap_err();
            assert!(error.contains("NETT API URL"), "{url:?}: {error}");
        }
        let local = resolve(
            &NETT,
            vars(&[("SUBSPACE_NETT_API_URL", "http://127.0.0.1:3000")]),
        )
        .unwrap();
        assert_eq!(local.nett_api_url, "http://127.0.0.1:3000");
    }

    #[test]
    fn an_unknown_stamped_backend_is_refused() {
        let other = Stamped {
            backend: "other",
            ..SUBSPACE
        };
        assert!(resolve(&other, Vec::new()).is_err());
    }
}
