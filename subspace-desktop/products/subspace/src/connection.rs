//! The connection file: `connection.json` beside the app's `config.json`,
//! which points a downloaded build at a hosted API and identity provider
//! without rebuilding. It holds the settings the `SUBSPACE_*` run-time
//! variables override ([`crate::launch`]), under their build config names,
//! with the same rules. A variable wins over the file, and the file over
//! the built-in value; a setting the file leaves out falls through. Without
//! the file nothing changes.
//!
//! It holds public addresses and client IDs only, never a secret. It cannot
//! change what the build fixed: its backend, branding or update source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;
use subspace_framework::product::ProductDescriptor;

use crate::launch::Failure;

/// The file's name, beside `config.json`.
pub(crate) const FILE_NAME: &str = "connection.json";

/// Each setting the file can hold, and the run-time variable it stands for.
pub(crate) const SETTINGS: [(&str, &str); 13] = [
    ("api_url", crate::launch::API_URL),
    ("nett_api_url", crate::launch::NETT_API_URL),
    ("identity_provider", crate::launch::PROVIDER),
    ("workos_client_id", crate::launch::WORKOS_CLIENT_ID),
    ("workos_api_url", crate::launch::WORKOS_API_URL),
    ("workos_redirect_port", crate::launch::WORKOS_REDIRECT_PORT),
    ("entra_tenant_id", crate::launch::ENTRA_TENANT_ID),
    ("entra_client_id", crate::launch::ENTRA_CLIENT_ID),
    ("entra_authority", crate::launch::ENTRA_AUTHORITY),
    ("entra_api_scope", crate::launch::ENTRA_API_SCOPE),
    ("dev_email", crate::launch::DEV_EMAIL),
    ("dev_name", crate::launch::DEV_NAME),
    ("dev_organisation", crate::launch::DEV_ORGANISATION),
];

/// Build settings the file names but cannot change.
const FIXED: [&str; 5] = [
    "api_backend",
    "organization_name",
    "title_prefix",
    "about_logo",
    "update",
];

/// The settings one connection file holds, as `(variable, value)` pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Connection {
    pub path: PathBuf,
    pub values: BTreeMap<&'static str, String>,
}

impl Connection {
    /// The settings as run-time variables, for [`crate::launch::resolve`].
    pub(crate) fn variables(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
    }
}

/// Reads the file at `path`: `None` when there is none, or why it cannot be
/// used, naming the setting.
pub(crate) fn read(path: &Path) -> Result<Option<Connection>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("The file cannot be read: {error}.")),
    };
    parse(&text).map(|values| {
        Some(Connection {
            path: path.to_owned(),
            values,
        })
    })
}

/// The settings in `text`: a JSON object of strings, the redirect port also
/// as a number.
pub(crate) fn parse(text: &str) -> Result<BTreeMap<&'static str, String>, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("The file is not valid JSON: {error}."))?;
    let Value::Object(entries) = value else {
        return Err("The file must hold one JSON object, such as {\"api_url\": \"https://api.example.com\"}.".into());
    };
    entries
        .iter()
        .map(|(key, value)| setting(key, value))
        .collect()
}

/// One entry: its variable and its value as text, or why it is refused.
fn setting(key: &str, value: &Value) -> Result<(&'static str, String), String> {
    let Some((_, variable)) = SETTINGS.iter().find(|(name, _)| *name == key) else {
        return Err(unknown(key));
    };
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) if key == "workos_redirect_port" => number.to_string(),
        _ => return Err(format!("The setting {key} must be text.")),
    };
    if key == "workos_redirect_port"
        && !text.trim().is_empty()
        && text.trim().parse::<u16>().is_err()
    {
        return Err(format!(
            "The setting {key} must be a port number from 1 to 65535."
        ));
    }
    Ok((variable, text))
}

fn unknown(key: &str) -> String {
    if FIXED.contains(&key) {
        format!("The setting {key} is fixed when the app is built; this file cannot change it.")
    } else {
        format!("{key} is not a setting this file holds.")
    }
}

/// Where this app keeps its connection file: beside its `config.json`.
/// None when the config folder cannot be found, so the app never reads a
/// file from wherever it was started.
pub(crate) fn locate(descriptor: &ProductDescriptor, version: &str) -> Option<PathBuf> {
    subspace_framework::shell::prepare_context(descriptor.clone(), version)
        .ok()
        .map(|context| subspace_framework::shell::config_file(&context).with_file_name(FILE_NAME))
}

/// Says why the app cannot start: in a window the desktop app shows until
/// the person quits, or on standard error for a command.
pub(crate) fn report(app_name: &str, failure: &Failure, desktop: bool) -> ExitCode {
    let title = format!("{app_name} cannot start");
    let detail = describe(failure);
    if desktop {
        return subspace_framework::shell::report_startup_failure(app_name, &title, &detail);
    }
    eprintln!("{detail}");
    ExitCode::FAILURE
}

/// The failure in words a person can act on: the file and setting, or the
/// run-time variable, and what to do.
pub(crate) fn describe(failure: &Failure) -> String {
    match failure {
        Failure::File(path, message) => format!(
            "The connection file {} has a problem: {message} Correct or remove the file, then open the app again.",
            path.display()
        ),
        Failure::Environment(message) => format!(
            "{message} The setting comes from a SUBSPACE_ environment variable or the app's build."
        ),
    }
}

/// `message`, from the launch rules, in the file's words: its setting
/// names instead of the variables'.
pub(crate) fn in_file_terms(message: &str) -> String {
    let mut message = message
        .replace("Unset ", "Remove ")
        .replace(" and start again", "");
    // Longest first, so SUBSPACE_API_URL never rewrites part of another.
    let mut names = SETTINGS;
    names.sort_by_key(|(_, variable)| std::cmp::Reverse(variable.len()));
    for (key, variable) in names {
        message = message
            .replace(variable, key)
            .replace(&format!("{key} or {key}"), key);
    }
    message
}

#[cfg(test)]
mod tests {
    use super::{in_file_terms, parse, read};

    #[test]
    fn the_file_holds_strings_under_the_build_config_names() {
        let values = parse(
            r#"{"api_url": "https://api.example.com", "identity_provider": "workos",
                "workos_client_id": "client_01ABC", "workos_redirect_port": 8787}"#,
        )
        .unwrap();
        assert_eq!(values["SUBSPACE_API_URL"], "https://api.example.com");
        assert_eq!(values["SUBSPACE_IDENTITY_PROVIDER"], "workos");
        assert_eq!(values["SUBSPACE_WORKOS_REDIRECT_PORT"], "8787");
        assert!(parse("{}").unwrap().is_empty());
    }

    #[test]
    fn a_bad_file_says_which_setting_and_why() {
        for (text, says) in [
            ("{", "not valid JSON"),
            ("[]", "one JSON object"),
            (
                r#"{"api_backend": "nett"}"#,
                "api_backend is fixed when the app is built",
            ),
            (r#"{"title_prefix": "Example"}"#, "title_prefix is fixed"),
            (
                r#"{"apiUrl": "https://api.example.com"}"#,
                "apiUrl is not a setting",
            ),
            (r#"{"api_url": 3}"#, "api_url must be text"),
            (r#"{"workos_redirect_port": "port"}"#, "port number"),
            (r#"{"workos_redirect_port": 70000}"#, "port number"),
        ] {
            let error = parse(text).unwrap_err();
            assert!(error.contains(says), "{text}: {error}");
        }
    }

    #[test]
    fn a_missing_file_is_no_file() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(read(&folder.path().join("connection.json")), Ok(None));
        let file = folder.path().join("connection.json");
        std::fs::write(&file, r#"{"api_url": "https://api.example.com"}"#).unwrap();
        let connection = read(&file).unwrap().unwrap();
        assert_eq!(connection.path, file);
        assert_eq!(connection.variables().count(), 1);
    }

    #[test]
    fn launch_messages_name_the_file_s_settings() {
        assert_eq!(
            in_file_terms("Unset SUBSPACE_API_URL, SUBSPACE_IDENTITY_PROVIDER and start again."),
            "Remove api_url, identity_provider."
        );
        assert_eq!(
            in_file_terms("(nett_api_url or SUBSPACE_NETT_API_URL)"),
            "(nett_api_url)"
        );
    }
}
