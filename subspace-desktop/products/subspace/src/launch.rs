//! Launch resolution: the settings the build stamped and the run's
//! `SUBSPACE_*` variables become one checked configuration before anything
//! is installed. It is a pure function of both, so tests can resolve either
//! build variant whatever the test binary itself was stamped with.
//!
//! A set variable wins over the stamped value, even when empty:
//! `SUBSPACE_IDENTITY_PROVIDER=` turns sign-in off for one run. A Subspace
//! API build refuses NETT settings and, signing in with WorkOS, an invalid
//! client ID. A NETT build refuses every Subspace API setting, signs in
//! only with WorkOS and needs a valid NETT API URL. A build with no API
//! backend refuses every API and sign-in setting. Every build names one of
//! the providers it signs in with, and builds that provider's
//! configuration here, so a value sign-in would refuse stops the launch.
//! Development sign-in with an API on another computer is not refused: the
//! app opens and says sign-in is unavailable.

use crate::connection::{Connection, blames_file, in_file_terms};
use crate::settings::{
    ApiBackend, refused_in_nett, refused_without_backend, valid_api_url, valid_workos_client_id,
};
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
/// API URL, a Subspace launch no NETT one, and a launch with no API backend
/// neither, nor a provider.
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
    /// Who the local dev issuer signs in.
    pub dev_user: subspace_api_client::DevUser,
}

/// Why a launch cannot start, and where the setting came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// A setting in the connection file at this path.
    File(std::path::PathBuf, String),
    /// A run-time variable, or the build itself.
    Environment(String),
}

/// Resolves the launch from the stamped settings, the connection file, when
/// there is one, and the run's variables: a variable wins over the file, and
/// the file over the stamped value, so a variable can complete or correct
/// the file. A failure is the file's when the setting it names took its
/// value from the file (`connection::blames_file`), and then in the file's
/// own words; otherwise it is the run's.
pub(crate) fn resolve_with(
    stamped: &Stamped,
    connection: Option<&Connection>,
    variables: impl IntoIterator<Item = (String, String)>,
) -> Result<Launch, Failure> {
    let variables: BTreeMap<String, String> = variables.into_iter().collect();
    let file: BTreeMap<String, String> = connection
        .map(|c| c.variables().collect())
        .unwrap_or_default();
    let mut merged = file.clone();
    merged.extend(variables.clone());
    resolve(stamped, merged).map_err(|message| {
        let file_alone_fails = || resolve(stamped, file.clone()).is_err();
        match connection.filter(|_| blames_file(&message, &file, &variables, file_alone_fails)) {
            Some(connection) => Failure::File(connection.path.clone(), in_file_terms(&message)),
            None => Failure::Environment(message),
        }
    })
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
        .ok_or_else(|| "The build's api_backend must be subspace, nett or none.".to_owned())?;
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
        dev_user: subspace_api_client::DevUser {
            email: value(DEV_EMAIL, stamped.dev_email),
            name: value(DEV_NAME, stamped.dev_name),
            organisation: value(DEV_ORGANISATION, stamped.dev_organisation),
        },
    };
    match backend {
        ApiBackend::Subspace => {
            check_subspace(&launch, &variables)?;
            launch.nett_api_url.clear();
        }
        ApiBackend::Nett => {
            check_nett(&launch, &variables)?;
            launch.api_url.clear();
        }
        // The build refused every API and sign-in setting and the run may
        // set none, so the launch holds no URL, provider or client.
        ApiBackend::None => check_without_backend(&variables)?,
    }
    Ok(launch)
}

/// A build with no API backend's rule: no API or sign-in variable, as a
/// NETT build refuses Subspace API's.
fn check_without_backend(variables: &BTreeMap<String, String>) -> Result<(), String> {
    let refused = refused_without_backend(
        variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    if refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "This build has no API backend and does not use API or sign-in settings. Unset {} and start again.",
        refused.join(", ")
    ))
}

/// A Subspace API build's rules: no NETT API URL, a valid Subspace API URL
/// when one is set, and a valid client ID when it signs in with WorkOS.
fn check_subspace(launch: &Launch, variables: &BTreeMap<String, String>) -> Result<(), String> {
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
    // The build refuses an invalid WorkOS client, and so does a run that
    // overrides it: a bad ID fails here, not at sign-in.
    if launch.provider == "workos" && !valid_workos_client_id(&launch.workos_client_id) {
        return Err(format!(
            "The workos provider requires a WorkOS client ID (workos_client_id or {WORKOS_CLIENT_ID}), such as client_01ABC."
        ));
    }
    crate::sign_in::check(launch)
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
    crate::sign_in::check(launch)
}

pub(crate) fn nett_url_error() -> String {
    format!(
        "The NETT API URL (nett_api_url or {NETT_API_URL}) must be HTTPS (or HTTP on a loopback address), optionally ending in /api, without credentials, query or fragment."
    )
}

fn subspace_url_error() -> String {
    format!(
        "The Subspace API URL (api_url or {API_URL}) must be HTTPS (or HTTP on a loopback address), optionally ending in /api, without credentials, query or fragment."
    )
}

/// Installs the launch's one backend before any Space starts. A NETT launch
/// installs only its NETT origin; an empty Subspace API URL leaves Issues and
/// the Channels initiative picker without a service. A launch with no API
/// backend installs that choice, so no origin or client can follow.
pub(crate) fn install(launch: &Launch) -> Result<(), String> {
    match launch.backend {
        ApiBackend::None => subspace_api_client::configure_no_backend()
            .map_err(|_| "This build has no API backend, but one was already configured.".into()),
        ApiBackend::Nett => {
            subspace_api_client::configure_nett_origin(&launch.nett_api_url)
                .map_err(|_| nett_url_error())?;
            subspace_api_client::nett::remember_organisations();
            Ok(())
        }
        ApiBackend::Subspace if launch.api_url.is_empty() => Ok(()),
        ApiBackend::Subspace => {
            if let Some(reason) = crate::sign_in::unavailable(launch) {
                subspace_api_client::set_sign_in_unavailable(reason);
            }
            subspace_api_client::configure_api_origin(&launch.api_url)
                .map_err(|_| subspace_url_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiBackend, Failure, Stamped, resolve, resolve_with};
    use crate::connection::{Connection, parse};

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
    fn a_subspace_build_signing_in_with_workos_checks_its_client_at_launch() {
        let workos = Stamped {
            provider: "workos",
            workos_client_id: CLIENT,
            ..SUBSPACE
        };
        assert_eq!(
            resolve(&workos, Vec::new()).unwrap().workos_client_id,
            CLIENT
        );
        for client in ["wrong_id", "", "client_", "client id"] {
            let error =
                resolve(&workos, vars(&[("SUBSPACE_WORKOS_CLIENT_ID", client)])).unwrap_err();
            assert!(error.contains("WorkOS client ID"), "{client:?}: {error}");
        }
        // Switching a dev build to WorkOS for one run needs a valid client.
        let switched = vars(&[("SUBSPACE_IDENTITY_PROVIDER", "workos")]);
        assert!(resolve(&SUBSPACE, switched).is_err());
        let switched = vars(&[
            ("SUBSPACE_IDENTITY_PROVIDER", "workos"),
            ("SUBSPACE_WORKOS_CLIENT_ID", CLIENT),
        ]);
        assert_eq!(resolve(&SUBSPACE, switched).unwrap().provider, "workos");
        // Other providers never read the WorkOS client.
        let dev = vars(&[("SUBSPACE_WORKOS_CLIENT_ID", "wrong_id")]);
        assert_eq!(resolve(&SUBSPACE, dev).unwrap().provider, "dev");
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

    /// A build with no API backend: nothing but the backend is stamped.
    const NONE: Stamped = Stamped {
        backend: "none",
        provider: "",
        api_url: "",
        nett_api_url: "",
        workos_client_id: "",
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

    #[test]
    fn a_build_without_a_backend_resolves_no_api_and_no_provider() {
        // Empty variables are not settings.
        let launch = resolve(
            &NONE,
            vars(&[
                ("HOME", "/tmp"),
                ("SUBSPACE_API_URL", ""),
                ("SUBSPACE_IDENTITY_PROVIDER", ""),
                ("SUBSPACE_GITHUB_API_URL", "http://127.0.0.1:9000"),
            ]),
        )
        .unwrap();
        assert_eq!(launch.backend, ApiBackend::None);
        assert_eq!(launch.provider, "", "no sign-in");
        assert_eq!(
            (launch.api_url.as_str(), launch.nett_api_url.as_str()),
            ("", "")
        );
        assert_eq!(launch.workos_client_id, "");
        assert_eq!(launch.dev_user.email, "");
    }

    #[test]
    fn a_build_without_a_backend_refuses_api_and_sign_in_overrides() {
        for (name, value) in [
            ("SUBSPACE_API_URL", "http://127.0.0.1:3000"),
            ("SUBSPACE_API_ACCESS_JWT_FILE", "/tmp/fixture-token"),
            ("SUBSPACE_NETT_API_URL", "https://api.nett.example"),
            ("SUBSPACE_IDENTITY_PROVIDER", "dev"),
            ("SUBSPACE_WORKOS_CLIENT_ID", CLIENT),
            ("SUBSPACE_DEV_EMAIL", "developer@example.com"),
            (
                "SUBSPACE_ENTRA_TENANT_ID",
                "00000000-0000-0000-0000-000000000001",
            ),
        ] {
            let refused = resolve(&NONE, vars(&[(name, value)])).unwrap_err();
            assert!(
                refused.contains("no API backend") && refused.contains(name),
                "{name}: {refused}"
            );
        }
        let both = resolve(
            &NONE,
            vars(&[
                ("SUBSPACE_IDENTITY_PROVIDER", "workos"),
                ("SUBSPACE_API_URL", "https://api.example.com"),
            ]),
        )
        .unwrap_err();
        assert!(
            both.contains("Unset SUBSPACE_API_URL, SUBSPACE_IDENTITY_PROVIDER and"),
            "{both}"
        );
    }

    fn file(json: &str) -> Connection {
        Connection {
            path: "/fixture/subspace/connection.json".into(),
            values: parse(json).unwrap(),
        }
    }

    #[test]
    fn a_variable_wins_over_the_file_and_the_file_over_the_build() {
        let hosted = file(
            r#"{"api_url": "https://api.example.com", "identity_provider": "workos",
                "workos_client_id": "client_01ABC"}"#,
        );
        let launch = resolve_with(&SUBSPACE, Some(&hosted), Vec::new()).unwrap();
        assert_eq!(launch.api_url, "https://api.example.com");
        assert_eq!(launch.provider, "workos");
        // A setting the file leaves out falls through to the build's.
        assert_eq!(launch.dev_user.email, "developer@example.com");
        let launch = resolve_with(
            &SUBSPACE,
            Some(&hosted),
            vars(&[("SUBSPACE_API_URL", "https://other.example.com")]),
        )
        .unwrap();
        assert_eq!(launch.api_url, "https://other.example.com");
        assert_eq!(launch.provider, "workos", "the file still sets the rest");
    }

    #[test]
    fn without_a_file_nothing_changes() {
        for stamped in [SUBSPACE, NETT, NONE] {
            assert_eq!(
                resolve_with(&stamped, None, Vec::new()).unwrap(),
                resolve(&stamped, Vec::new()).unwrap()
            );
        }
        let empty = file("{}");
        assert_eq!(
            resolve_with(&SUBSPACE, Some(&empty), Vec::new()).unwrap(),
            resolve(&SUBSPACE, Vec::new()).unwrap()
        );
    }

    #[test]
    fn the_file_is_held_to_the_same_rules_and_named_when_it_breaks_one() {
        for (json, says) in [
            (r#"{"api_url": "http://api.example.com"}"#, "must be HTTPS"),
            (r#"{"identity_provider": "workos"}"#, "workos_client_id"),
            (
                r#"{"identity_provider": "workos", "workos_client_id": "wrong"}"#,
                "workos_client_id",
            ),
            (
                r#"{"nett_api_url": "https://api.nett.example"}"#,
                "Remove nett_api_url",
            ),
            (r#"{"identity_provider": "WorkOS"}"#, "identity_provider"),
            (
                r#"{"identity_provider": "workos", "workos_client_id": "client_01ABC",
                    "workos_api_url": "not a url"}"#,
                "(workos_api_url)",
            ),
            (
                r#"{"identity_provider": "entra", "entra_tenant_id": "x", "entra_client_id": "y"}"#,
                "tenant ID (entra_tenant_id) or client ID (entra_client_id)",
            ),
        ] {
            match resolve_with(&SUBSPACE, Some(&file(json)), Vec::new()) {
                Err(Failure::File(path, message)) => {
                    assert!(path.ends_with("connection.json"));
                    assert!(message.contains(says), "{json}: {message}");
                    assert!(!message.contains("SUBSPACE_"), "{json}: {message}");
                }
                other => panic!("{json}: {other:?}"),
            }
        }
        // A bad variable over a good file is the variable's.
        let failure = resolve_with(
            &SUBSPACE,
            Some(&file(r#"{"api_url": "http://127.0.0.1:3000"}"#)),
            vars(&[("SUBSPACE_API_URL", "http://api.example.com")]),
        )
        .unwrap_err();
        assert!(matches!(failure, Failure::Environment(_)), "{failure:?}");
    }

    #[test]
    fn a_variable_can_complete_or_correct_the_file() {
        // The file alone names WorkOS without its client.
        let launch = resolve_with(
            &SUBSPACE,
            Some(&file(
                r#"{"api_url": "https://api.example.com", "identity_provider": "workos"}"#,
            )),
            vars(&[("SUBSPACE_WORKOS_CLIENT_ID", "client_01ABC")]),
        )
        .unwrap();
        assert_eq!(launch.api_url, "https://api.example.com");
        assert_eq!(launch.provider, "workos");
        let launch = resolve_with(
            &SUBSPACE,
            Some(&file(r#"{"api_url": "http://api.example.com"}"#)),
            vars(&[("SUBSPACE_API_URL", "http://127.0.0.1:3000")]),
        )
        .unwrap();
        assert_eq!(launch.api_url, "http://127.0.0.1:3000");
        // An empty variable still turns the file's sign-in off for one run.
        let launch = resolve_with(
            &SUBSPACE,
            Some(&file(
                r#"{"identity_provider": "workos", "workos_client_id": "client_01ABC"}"#,
            )),
            vars(&[("SUBSPACE_IDENTITY_PROVIDER", "")]),
        )
        .unwrap();
        assert_eq!(launch.provider, "");
    }

    #[test]
    fn a_provider_value_sign_in_would_refuse_stops_the_launch_by_its_variable() {
        for (name, value, says) in [
            (
                "SUBSPACE_IDENTITY_PROVIDER",
                "WorkOS",
                "SUBSPACE_IDENTITY_PROVIDER",
            ),
            (
                "SUBSPACE_WORKOS_API_URL",
                "not a url",
                "SUBSPACE_WORKOS_API_URL",
            ),
            (
                "SUBSPACE_WORKOS_REDIRECT_PORT",
                "eighty",
                "SUBSPACE_WORKOS_REDIRECT_PORT",
            ),
        ] {
            let failure = resolve_with(
                &SUBSPACE,
                None,
                vars(&[
                    ("SUBSPACE_IDENTITY_PROVIDER", "workos"),
                    ("SUBSPACE_WORKOS_CLIENT_ID", "client_01ABC"),
                    (name, value),
                ]),
            )
            .unwrap_err();
            match failure {
                Failure::Environment(message) => assert!(message.contains(says), "{message}"),
                other => panic!("{name}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_failure_is_blamed_on_where_its_setting_came_from() {
        let workos = file(r#"{"identity_provider": "workos"}"#);
        // The file leaves the client to a variable, and the variable is wrong.
        let failure = resolve_with(
            &SUBSPACE,
            Some(&workos),
            vars(&[("SUBSPACE_WORKOS_CLIENT_ID", "wrong")]),
        )
        .unwrap_err();
        assert!(matches!(failure, Failure::Environment(_)), "{failure:?}");
        // A variable completes the file, and a variable elsewhere is wrong.
        let failure = resolve_with(
            &SUBSPACE,
            Some(&workos),
            vars(&[
                ("SUBSPACE_WORKOS_CLIENT_ID", "client_01ABC"),
                ("SUBSPACE_API_URL", "http://api.example.com"),
            ]),
        )
        .unwrap_err();
        match failure {
            Failure::Environment(message) => {
                assert!(message.contains("SUBSPACE_API_URL"), "{message}");
                assert!(!message.contains("client"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        // A variable completes the file, and the file's own value is wrong.
        let failure = resolve_with(
            &SUBSPACE,
            Some(&file(
                r#"{"identity_provider": "workos", "workos_api_url": "not a url"}"#,
            )),
            vars(&[("SUBSPACE_WORKOS_CLIENT_ID", "client_01ABC")]),
        )
        .unwrap_err();
        match failure {
            Failure::File(_, message) => assert!(message.contains("(workos_api_url)"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_variable_corrects_a_mistyped_value_in_the_file() {
        let mistyped = file(
            r#"{"identity_provider": "workos", "workos_client_id": "client_01ABC",
                "workos_redirect_port": "eight"}"#,
        );
        match resolve_with(&SUBSPACE, Some(&mistyped), Vec::new()) {
            Err(Failure::File(_, message)) => {
                assert!(message.contains("(workos_redirect_port)"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        let launch = resolve_with(
            &SUBSPACE,
            Some(&mistyped),
            vars(&[("SUBSPACE_WORKOS_REDIRECT_PORT", "8400")]),
        )
        .unwrap();
        assert_eq!(launch.workos_redirect_port, "8400");
    }

    #[test]
    fn entra_with_an_api_needs_its_scope() {
        let entra = r#"{"identity_provider": "entra",
            "entra_tenant_id": "00000000-0000-0000-0000-000000000001",
            "entra_client_id": "00000000-0000-0000-0000-000000000002"}"#;
        match resolve_with(&SUBSPACE, Some(&file(entra)), Vec::new()) {
            Err(Failure::File(_, message)) => {
                assert!(message.contains("(entra_api_scope)"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        let scope = (
            "SUBSPACE_ENTRA_API_SCOPE",
            "api://00000000-0000-0000-0000-000000000003/API.Access",
        );
        assert!(resolve_with(&SUBSPACE, Some(&file(entra)), vars(&[scope])).is_ok());
        let cleared = resolve_with(
            &SUBSPACE,
            Some(&file(&entra.replace(
                "\"entra_client_id\"",
                "\"entra_api_scope\": \"api://00000000-0000-0000-0000-000000000003/API.Access\", \"entra_client_id\"",
            ))),
            vars(&[("SUBSPACE_ENTRA_API_SCOPE", "")]),
        )
        .unwrap_err();
        assert!(matches!(cleared, Failure::Environment(_)), "{cleared:?}");
    }

    #[test]
    fn development_sign_in_on_a_hosted_api_opens_without_sign_in() {
        let hosted = vars(&[("SUBSPACE_API_URL", "https://api.example.com")]);
        let launch = resolve_with(&SUBSPACE, None, hosted).unwrap();
        let reason = crate::sign_in::unavailable(&launch).unwrap();
        assert!(reason.contains("https://api.example.com"), "{reason}");
        let launch = resolve_with(
            &SUBSPACE,
            Some(&file(r#"{"api_url": "https://api.example.com"}"#)),
            Vec::new(),
        )
        .unwrap();
        assert!(crate::sign_in::unavailable(&launch).is_some());
        let local = resolve(&SUBSPACE, Vec::new()).unwrap();
        assert_eq!(crate::sign_in::unavailable(&local), None);
    }

    #[test]
    fn a_build_without_a_backend_refuses_a_file_that_would_switch_an_api_on() {
        for json in [
            r#"{"api_url": "https://api.example.com"}"#,
            r#"{"identity_provider": "workos", "workos_client_id": "client_01ABC"}"#,
        ] {
            match resolve_with(&NONE, Some(&file(json)), Vec::new()) {
                Err(Failure::File(_, message)) => {
                    assert!(message.contains("no API backend"), "{message}");
                }
                other => panic!("{json}: {other:?}"),
            }
        }
        // Empty settings switch nothing on.
        assert!(resolve_with(&NONE, Some(&file(r#"{"api_url": ""}"#)), Vec::new()).is_ok());
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
