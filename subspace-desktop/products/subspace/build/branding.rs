//! Non-secret build settings: the identity provider, the Subspace API URL and
//! optional organisation branding. A build embeds the selected config and
//! logo; `src/identity.rs` lets the same-named environment variables override
//! the provider settings at run time.

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_ENV: &str = "SUBSPACE_BUILD_CONFIG";
pub(crate) const DEFAULT_CONFIG: &str = "build-config.json";
const LOGO_OUTPUT: &str = "organization_logo.rs";
const PROVIDERS: [&str; 4] = ["", "dev", "workos", "entra"];

#[derive(serde::Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Branding {
    identity_provider: String,
    api_url: String,
    workos_client_id: String,
    workos_api_url: String,
    workos_redirect_port: Option<u16>,
    entra_tenant_id: String,
    entra_client_id: String,
    entra_authority: String,
    entra_api_scope: String,
    dev_email: String,
    dev_name: String,
    dev_organisation: String,
    organization_name: String,
    title_prefix: String,
    about_logo: Option<PathBuf>,
}

pub(crate) fn embed(path: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed={CONFIG_ENV}");
    println!("cargo:rerun-if-changed={}", path.display());
    let branding: Branding = serde_json::from_slice(&fs::read(path)?)?;
    for (name, value) in branding.values()? {
        println!("cargo:rustc-env=SUBSPACE_BUILD_{name}={value}");
    }
    let logo = read_logo(
        branding
            .about_logo
            .as_ref()
            .map(|logo| path.with_file_name(logo)),
    )?;
    fs::write(
        output.join(LOGO_OUTPUT),
        format!("const ORGANIZATION_LOGO: &[u8] = &{logo:?};\n"),
    )?;
    Ok(())
}

impl Branding {
    fn values(&self) -> Result<Vec<(&'static str, String)>, &'static str> {
        let port = self
            .workos_redirect_port
            .map(|port| port.to_string())
            .unwrap_or_default();
        let values = vec![
            (
                "IDENTITY_PROVIDER",
                self.identity_provider.trim().to_owned(),
            ),
            ("API_URL", self.api_url.trim().to_owned()),
            ("WORKOS_CLIENT_ID", self.workos_client_id.trim().to_owned()),
            ("WORKOS_API_URL", self.workos_api_url.trim().to_owned()),
            ("WORKOS_REDIRECT_PORT", port),
            ("ENTRA_TENANT_ID", self.entra_tenant_id.trim().to_owned()),
            ("ENTRA_CLIENT_ID", self.entra_client_id.trim().to_owned()),
            ("ENTRA_AUTHORITY", self.entra_authority.trim().to_owned()),
            ("ENTRA_API_SCOPE", self.entra_api_scope.trim().to_owned()),
            ("DEV_EMAIL", self.dev_email.trim().to_owned()),
            ("DEV_NAME", self.dev_name.trim().to_owned()),
            ("DEV_ORGANISATION", self.dev_organisation.trim().to_owned()),
            (
                "ORGANIZATION_NAME",
                self.organization_name.trim().to_owned(),
            ),
            ("TITLE_PREFIX", self.title_prefix.trim().to_owned()),
        ];
        if values
            .iter()
            .any(|(_, value)| value.chars().any(char::is_control))
        {
            return Err("Build config values must fit on one line");
        }
        validate(&values)?;
        Ok(values)
    }
}

fn value<'a>(values: &'a [(&str, String)], name: &str) -> &'a str {
    values
        .iter()
        .find(|(key, _)| *key == name)
        .map_or("", |(_, value)| value.as_str())
}

fn validate(values: &[(&str, String)]) -> Result<(), &'static str> {
    let provider = value(values, "IDENTITY_PROVIDER");
    if !PROVIDERS.contains(&provider) {
        return Err("identity_provider must be dev, workos, entra or empty");
    }
    if !valid_url(value(values, "API_URL")) {
        return Err("api_url must be HTTPS, or HTTP on a loopback address");
    }
    match provider {
        "entra"
            if !valid_id(value(values, "ENTRA_TENANT_ID"))
                || !valid_id(value(values, "ENTRA_CLIENT_ID")) =>
        {
            Err("The entra provider requires non-nil tenant and public client UUIDs")
        }
        "workos" if value(values, "WORKOS_CLIENT_ID").is_empty() => {
            Err("The workos provider requires workos_client_id")
        }
        "dev" if value(values, "API_URL").is_empty() => Err("The dev provider requires api_url"),
        _ => Ok(()),
    }
}

fn valid_url(url: &str) -> bool {
    url.is_empty()
        || url.starts_with("https://")
        || ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|prefix| {
                url.strip_prefix(prefix)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
            })
}

fn valid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}

fn read_logo(path: Option<PathBuf>) -> std::io::Result<Vec<u8>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    println!("cargo:rerun-if-changed={}", path.display());
    if path.extension().and_then(std::ffi::OsStr::to_str) != Some("svg") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The About logo must be an SVG file",
        ));
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The About logo SVG is empty",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Branding, LOGO_OUTPUT, embed, fs, valid_url};

    fn branding(json: &str) -> Result<Vec<(&'static str, String)>, &'static str> {
        serde_json::from_str::<Branding>(json).unwrap().values()
    }

    #[test]
    fn providers_require_their_own_settings() {
        const ID: &str = "00000000-0000-0000-0000-000000000001";
        assert!(branding("{}").is_ok());
        assert!(
            branding(r#"{"identity_provider":"dev","api_url":"http://127.0.0.1:3000"}"#).is_ok()
        );
        assert!(branding(r#"{"identity_provider":"dev"}"#).is_err());
        assert!(branding(r#"{"identity_provider":"workos"}"#).is_err());
        assert!(
            branding(r#"{"identity_provider":"workos","workos_client_id":"client_x"}"#).is_ok()
        );
        assert!(branding(r#"{"identity_provider":"entra","entra_tenant_id":"common"}"#).is_err());
        let entra = format!(
            r#"{{"identity_provider":"entra","entra_tenant_id":"{ID}","entra_client_id":"{ID}"}}"#
        );
        assert!(branding(&entra).is_ok());
        assert!(branding(r#"{"identity_provider":"okta"}"#).is_err());
        assert!(branding(r#"{"title_prefix":"Line\nbreak"}"#).is_err());
    }

    #[test]
    fn the_api_url_is_https_or_loopback_http() {
        for (url, valid) in [
            ("", true),
            ("https://api.example.com", true),
            ("http://127.0.0.1:3000", true),
            ("http://localhost/api", true),
            ("http://127.0.0.1.example.com", false),
            ("http://api.example.com", false),
        ] {
            assert_eq!(valid_url(url), valid, "{url}");
        }
    }

    #[test]
    fn embeds_relative_logo_and_clears_it_when_absent() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join(super::DEFAULT_CONFIG);
        fs::write(root.path().join("fixture.svg"), b"<svg/>").unwrap();
        fs::write(&config, r#"{"about_logo":"fixture.svg"}"#).unwrap();
        embed(&config, root.path()).unwrap();
        let logo = root.path().join(LOGO_OUTPUT);
        assert_eq!(
            fs::read_to_string(&logo).unwrap(),
            "const ORGANIZATION_LOGO: &[u8] = &[60, 115, 118, 103, 47, 62];\n"
        );
        fs::write(&config, "{}").unwrap();
        embed(&config, root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(logo).unwrap(),
            "const ORGANIZATION_LOGO: &[u8] = &[];\n"
        );
    }
}
