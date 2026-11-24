//! Secrets read at connect time, so they live only in the session's memory:
//! the SASL password from the settings, else from its password file, else
//! a login fetched from the network's credential source. An issued password
//! stays apart from the configuration, for the session's one SASL exchange.

use crate::model::{CredentialSource, IssuedLogin, IssuedPassword, NetworkConfig};

/// Why a connect could not get its secrets.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Unresolved {
    /// The settings are wrong, such as a password file that cannot be
    /// read: retrying would not help.
    Fatal(String),
    /// The credential source could not issue a login now; the connection
    /// retries with its backoff.
    Retry(String),
}

/// What a connect signs in with: the configuration, registering with an
/// issued nick and SASL username, and an issued password apart from it.
#[derive(Debug)]
pub(super) struct Resolved {
    pub config: NetworkConfig,
    pub issued: Option<IssuedPassword>,
}

/// `config` with its SASL login for this connect. Blocking work runs on a
/// blocking thread.
pub(super) async fn resolve(config: &NetworkConfig) -> Result<Resolved, Unresolved> {
    let config = from_file(config).map_err(Unresolved::Fatal)?;
    let login = fetch(pending_source(&config))
        .await
        .map_err(Unresolved::Retry)?;
    Ok(with_login(config, login))
}

/// `config` with the SASL password read from `password_file` when the
/// password itself is empty. Trailing line breaks are not part of it.
fn from_file(config: &NetworkConfig) -> Result<NetworkConfig, String> {
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

/// The credential source to ask, when PLAIN still has no password.
pub(super) fn pending_source(config: &NetworkConfig) -> Option<CredentialSource> {
    config
        .sasl
        .as_ref()
        .filter(|sasl| sasl.password.is_empty() && !sasl.external)
        .and_then(|sasl| sasl.credential.clone())
}

/// A login from `source`, fetched on a blocking thread.
async fn fetch(source: Option<CredentialSource>) -> Result<Option<IssuedLogin>, String> {
    let Some(source) = source else {
        return Ok(None);
    };
    tokio::task::spawn_blocking(move || source.fetch())
        .await
        .map_err(|_| "the sign-in could not be fetched".to_owned())?
        .map(Some)
}

/// `config` registering with the issued nickname and SASL username, and
/// the issued password apart from it.
fn with_login(mut config: NetworkConfig, login: Option<IssuedLogin>) -> Resolved {
    let (Some(login), Some(sasl)) = (login, config.sasl.as_mut()) else {
        return Resolved {
            config,
            issued: None,
        };
    };
    sasl.username = login.username;
    if !login.nickname.is_empty() {
        config.nickname = login.nickname;
    }
    Resolved {
        config,
        issued: Some(login.password),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Unresolved, from_file, resolve};
    use crate::model::{CredentialSource, IssuedLogin, IssuedPassword, NetworkConfig, Sasl};

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
            credential: None,
        })
    }

    /// A source that issues `ada` and counts how often it was asked.
    fn issuing(asked: &Arc<AtomicUsize>) -> CredentialSource {
        let asked = Arc::clone(asked);
        CredentialSource::new("fixture-api", move || {
            asked.fetch_add(1, Ordering::SeqCst);
            Ok(IssuedLogin {
                nickname: "ada".into(),
                username: "ada@desk1".into(),
                password: IssuedPassword::new("issued-fixture"),
            })
        })
    }

    #[test]
    fn a_password_file_fills_an_empty_password_without_its_line_break() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "s3cret").unwrap();
        let resolved = from_file(&config(sasl("", Some(file.path().into())))).unwrap();
        assert_eq!(resolved.sasl.unwrap().password, "s3cret");
    }

    #[test]
    fn an_inline_password_wins_and_no_sasl_is_unchanged() {
        let inline = from_file(&config(sasl("inline", Some("/missing".into())))).unwrap();
        assert_eq!(inline.sasl.unwrap().password, "inline");
        assert!(from_file(&config(None)).unwrap().sasl.is_none());
    }

    #[test]
    fn a_missing_file_names_the_file_but_not_a_secret() {
        let error = from_file(&config(sasl("", Some("/no/such/irc.password".into())))).unwrap_err();
        assert!(error.contains("/no/such/irc.password"));
    }

    #[tokio::test]
    async fn a_credential_source_gives_the_nick_and_login_for_each_connect() {
        let asked = Arc::new(AtomicUsize::new(0));
        let mut settings = sasl("", None);
        if let Some(sasl) = settings.as_mut() {
            sasl.credential = Some(issuing(&asked));
        }
        let config = config(settings);
        for connect in 1..=2 {
            let resolved = resolve(&config).await.unwrap();
            assert_eq!(
                resolved.config.nickname, "ada",
                "the issued account is the nick"
            );
            let sasl = resolved.config.sasl.unwrap();
            assert_eq!(sasl.username, "ada@desk1");
            assert!(
                sasl.password.is_empty(),
                "the issued password stays out of the configuration"
            );
            assert_eq!(resolved.issued.unwrap().expose(), "issued-fixture");
            assert_eq!(
                asked.load(Ordering::SeqCst),
                connect,
                "asked at every connect"
            );
        }
        assert_eq!(
            config.sasl.as_ref().map(|sasl| sasl.password.as_str()),
            Some(""),
            "the settings never keep the issued password"
        );
    }

    #[tokio::test]
    async fn a_saved_password_or_external_login_never_asks_the_source() {
        let asked = Arc::new(AtomicUsize::new(0));
        for (password, external) in [("inline", false), ("", true)] {
            let mut settings = sasl(password, None);
            if let Some(sasl) = settings.as_mut() {
                sasl.external = external;
                sasl.credential = Some(issuing(&asked));
            }
            let resolved = resolve(&config(settings)).await.unwrap();
            assert_eq!(resolved.config.nickname, "zack");
            assert!(resolved.issued.is_none());
        }
        assert_eq!(asked.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_source_that_cannot_issue_asks_for_a_retry_and_a_bad_file_does_not() {
        let mut settings = sasl("", None);
        if let Some(sasl) = settings.as_mut() {
            sasl.credential = Some(CredentialSource::new("fixture-api", || {
                Err("the API could not be reached".into())
            }));
        }
        assert_eq!(
            resolve(&config(settings)).await.unwrap_err(),
            Unresolved::Retry("the API could not be reached".into())
        );
        let file = config(sasl("", Some("/no/such/irc.password".into())));
        assert!(matches!(
            resolve(&file).await.unwrap_err(),
            Unresolved::Fatal(_)
        ));
    }
}
