//! Secrets read at connect time, so they live only in the session's memory.

use crate::model::NetworkConfig;

/// `config` with the SASL password read from `password_file` when the
/// password itself is empty. Trailing line breaks are not part of it.
pub(super) fn resolve(config: &NetworkConfig) -> Result<NetworkConfig, String> {
    let mut config = config.clone();
    let Some(sasl) = config.sasl.as_mut() else {
        return Ok(config);
    };
    let Some(file) = sasl
        .password_file
        .as_ref()
        .filter(|_| sasl.password.is_empty())
    else {
        return Ok(config);
    };
    let text = std::fs::read_to_string(file).map_err(|error| {
        format!(
            "cannot read the SASL password file {}: {error}",
            file.display()
        )
    })?;
    sasl.password = text.trim_end_matches(['\r', '\n']).to_owned();
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::resolve;
    use crate::model::{NetworkConfig, Sasl};

    fn config(sasl: Option<Sasl>) -> NetworkConfig {
        serde_json::from_value::<NetworkConfig>(serde_json::json!({
            "id": "fleet", "host": "irc.example.test", "nickname": "zack"
        }))
        .map(|config| NetworkConfig { sasl, ..config })
        .unwrap()
    }

    fn sasl(password: &str, file: Option<std::path::PathBuf>) -> Option<Sasl> {
        Some(Sasl {
            username: "zack".into(),
            password: password.into(),
            password_file: file,
            external: false,
        })
    }

    #[test]
    fn a_password_file_fills_an_empty_password_without_its_line_break() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "s3cret").unwrap();
        let resolved = resolve(&config(sasl("", Some(file.path().into())))).unwrap();
        assert_eq!(resolved.sasl.unwrap().password, "s3cret");
    }

    #[test]
    fn an_inline_password_wins_and_no_sasl_is_unchanged() {
        let inline = resolve(&config(sasl("inline", Some("/missing".into())))).unwrap();
        assert_eq!(inline.sasl.unwrap().password, "inline");
        assert!(resolve(&config(None)).unwrap().sasl.is_none());
    }

    #[test]
    fn a_missing_file_names_the_file_but_not_a_secret() {
        let error = resolve(&config(sasl("", Some("/no/such/irc.password".into())))).unwrap_err();
        assert!(error.contains("/no/such/irc.password"));
    }
}
