//! Sign-in settings: the provider's configuration, built from a launch the
//! same way at start-up, to refuse a value sign-in could not use, and when
//! signing in, so the two never disagree. Pure, so the launch tests reach it.

use crate::launch::{
    ENTRA_API_SCOPE, ENTRA_AUTHORITY, ENTRA_CLIENT_ID, ENTRA_TENANT_ID, Launch, PROVIDER,
    WORKOS_API_URL, WORKOS_CLIENT_ID, WORKOS_REDIRECT_PORT,
};
use crate::settings::loopback_url;
use subspace_framework::{entra_auth, workos_auth};

/// The Subspace API's resource name in sign-in requests.
pub(crate) const API_RESOURCE: &str = "subspace-api";

/// The provider is one this app signs in with, and its configuration is
/// built here as sign-in builds it, so a value sign-in would refuse stops
/// the launch, named by its setting, rather than silently turning sign-in
/// off.
pub(crate) fn check(launch: &Launch) -> Result<(), String> {
    match launch.provider.as_str() {
        "" | "dev" => Ok(()),
        "workos" => workos_config(launch).map(drop),
        "entra" => entra_config(launch).map(drop),
        other => Err(format!(
            "The identity provider (identity_provider or {PROVIDER}) must be dev, workos, entra or empty, not \"{other}\"."
        )),
    }
}

/// The WorkOS configuration the launch selects, with its client, API and
/// redirect port; an error names the setting that gave the value.
pub(crate) fn workos_config(launch: &Launch) -> Result<workos_auth::Config, String> {
    let mut config = workos_auth::Config::new(&launch.workos_client_id).map_err(|_| {
        unusable(
            "WorkOS client ID",
            "workos_client_id",
            WORKOS_CLIENT_ID,
            "client_01ABC",
        )
    })?;
    if !launch.workos_api_url.is_empty() {
        config = config.with_api_url(&launch.workos_api_url).map_err(|_| {
            unusable(
                "WorkOS API URL",
                "workos_api_url",
                WORKOS_API_URL,
                "https://api.workos.com",
            )
        })?;
    }
    if !launch.workos_redirect_port.is_empty() {
        let port = launch.workos_redirect_port.parse().map_err(|_| {
            unusable(
                "WorkOS redirect port",
                "workos_redirect_port",
                WORKOS_REDIRECT_PORT,
                "8400",
            )
        })?;
        config = config.with_redirect_port(port);
    }
    Ok(config)
}

/// The Entra configuration the launch selects, with its tenant, client,
/// authority and API scope; an error names the setting that gave the value.
pub(crate) fn entra_config(launch: &Launch) -> Result<entra_auth::Config, String> {
    let mut config = entra_auth::Config::new(&launch.entra_tenant_id, &launch.entra_client_id)
        .map_err(|_| {
            format!(
                "The Entra tenant ID (entra_tenant_id or {ENTRA_TENANT_ID}) or client ID (entra_client_id or {ENTRA_CLIENT_ID}) is not one sign-in can use; set each like 00000000-0000-0000-0000-000000000001."
            )
        })?;
    if !launch.entra_authority.is_empty() {
        config = config
            .with_authority(&launch.entra_authority)
            .map_err(|_| {
                let example = "https://login.microsoftonline.com";
                unusable(
                    "Entra authority",
                    "entra_authority",
                    ENTRA_AUTHORITY,
                    example,
                )
            })?;
    }
    if !launch.entra_api_scope.is_empty() {
        config = config
            .with_scope(API_RESOURCE, &launch.entra_api_scope)
            .map_err(|_| {
                let example = "api://00000000-0000-0000-0000-000000000003/API.Access";
                unusable(
                    "Entra API scope",
                    "entra_api_scope",
                    ENTRA_API_SCOPE,
                    example,
                )
            })?;
    }
    Ok(config)
}

/// A setting sign-in cannot use, by its build config and variable names.
fn unusable(what: &str, key: &str, variable: &str, example: &str) -> String {
    format!("The {what} ({key} or {variable}) is not one sign-in can use; set it like {example}.")
}

/// Why this launch cannot sign in at all although it names a provider:
/// development sign-in reaches only an API on this computer. The app still
/// opens, and the Spaces say so instead of asking for a sign-in.
pub(crate) fn unavailable(launch: &Launch) -> Option<String> {
    let hosted = !launch.api_url.is_empty() && !loopback_url(&launch.api_url);
    (launch.provider == "dev" && hosted).then(|| {
        format!(
            "Development sign-in reaches only an API on this computer, and this app is set to {}. Set identity_provider to workos or entra, or api_url to this computer, in connection.json, then open the app again.",
            launch.api_url
        )
    })
}
