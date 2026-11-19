//! The GitHub token for one update operation (REQ-03), resolved once and
//! never printed.

use std::time::Duration;

use anyhow::{Context, Result};
use subspace_core::Secret;
use subspace_core::credentials::{Environment, Runner};
use subspace_product::UpdateSource;

/// `gh` is a user-facing login helper; a long wait is worse than an error.
const GH_TIMEOUT: Duration = Duration::from_secs(15);
const GH_ARGS: [&str; 4] = ["auth", "token", "--hostname", "github.com"];
/// Variables every GitHub tool reads, after the app's own variable.
const SHARED_VARIABLES: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// A GitHub token. Neither `Debug` nor `Display` reveals the value, so no
/// error, context chain or log line can carry it.
#[derive(Clone)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn new(value: &str) -> Self {
        Self(value.to_owned())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Token(redacted)")
    }
}

/// The trimmed value, or `None` for a missing, empty or whitespace one.
fn present(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Reads the explicit token variables first. A chosen account is used for
/// both Subspace and gh lookup. With no choice, use the default credentials.
pub(crate) fn resolve(
    source: &UpdateSource,
    selected: Option<&str>,
    env: &dyn Environment,
    signed_in: &dyn Fn(Option<&str>) -> Option<Secret>,
    runner: &dyn Runner,
) -> Result<Token> {
    let names = [source.token_variable.as_str()];
    let from_env = names
        .into_iter()
        .chain(SHARED_VARIABLES)
        .find_map(|name| present(env.var(name)));
    let selected = selected.map(str::trim).filter(|login| !login.is_empty());
    from_env
        .or_else(|| match selected {
            Some(login) => selected_token(login, signed_in, runner),
            None => signed_in(None)
                .and_then(secret_value)
                .or_else(|| present(runner.run("gh", &GH_ARGS, &[], GH_TIMEOUT).ok())),
        })
        .map(|value| Token::new(&value))
        .with_context(|| missing_token(source, selected))
}

fn missing_token(source: &UpdateSource, selected: Option<&str>) -> String {
    if let Some(login) = selected {
        return format!(
            "No GitHub token for {login}. Sign in with that account in Settings or run gh auth login."
        );
    }
    format!(
        "{} is private; sign in with GitHub in Settings, run gh auth login or set {} to a token that can read it",
        source.repository, source.token_variable
    )
}

fn selected_token(
    login: &str,
    signed_in: &dyn Fn(Option<&str>) -> Option<Secret>,
    runner: &dyn Runner,
) -> Option<String> {
    signed_in(Some(login)).and_then(secret_value).or_else(|| {
        let [auth, token, host_flag, hostname] = GH_ARGS;
        let args = [auth, token, host_flag, hostname, "--user", login];
        present(runner.run("gh", &args, &SHARED_VARIABLES, GH_TIMEOUT).ok())
    })
}

fn secret_value(secret: Secret) -> Option<String> {
    present(Some(secret.expose().to_owned()))
}

pub(crate) fn signed_in_token(selected: Option<&str>) -> Option<Secret> {
    let accounts = subspace_core::credentials::SignedInAccounts::system();
    let login = match selected {
        Some(login) => login.to_owned(),
        None => accounts.preferred(subspace_core::credentials::saved_account().as_deref())?,
    };
    accounts.token(&login).ok().flatten()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use subspace_core::Secret;
    use subspace_core::credentials::{Environment, Runner};
    use subspace_product::UpdateSource;

    use super::{Token, resolve};

    fn no_sign_in(_: Option<&str>) -> Option<Secret> {
        None
    }

    struct FakeEnv(BTreeMap<String, String>);

    impl Environment for FakeEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    struct FakeGh(Option<&'static str>);

    impl Runner for FakeGh {
        fn run(
            &self,
            _program: &str,
            _args: &[&str],
            _remove: &[&str],
            _timeout: Duration,
        ) -> Result<String, String> {
            self.0.map(str::to_owned).ok_or_else(|| "no gh".to_owned())
        }
    }

    fn source() -> UpdateSource {
        UpdateSource {
            repository: "owner/name".into(),
            tag_prefix: "tools/app/v".into(),
            archive_prefix: "app".into(),
            app_bundle: "App".into(),
            token_variable: "APP_GITHUB_TOKEN".into(),
            channel: subspace_product::Channel::Stable,
        }
    }

    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn the_app_variable_wins_then_gh_token_then_github_token_then_gh() {
        let cases: [(&[(&str, &str)], &str); 4] = [
            (
                &[
                    ("APP_GITHUB_TOKEN", "app"),
                    ("GH_TOKEN", "gh"),
                    ("GITHUB_TOKEN", "github"),
                ],
                "app",
            ),
            (&[("GH_TOKEN", "gh"), ("GITHUB_TOKEN", "github")], "gh"),
            (&[("GITHUB_TOKEN", "github")], "github"),
            (&[], "from-gh"),
        ];
        for (pairs, expected) in cases {
            let token = resolve(
                &source(),
                None,
                &env(pairs),
                &no_sign_in,
                &FakeGh(Some("  from-gh\n")),
            )
            .unwrap();
            assert_eq!(token.expose(), expected);
        }
    }

    #[test]
    fn an_empty_or_whitespace_variable_counts_as_unset() {
        let token = resolve(
            &source(),
            None,
            &env(&[
                ("APP_GITHUB_TOKEN", ""),
                ("GH_TOKEN", "   "),
                ("GITHUB_TOKEN", " github \n"),
            ]),
            &no_sign_in,
            &FakeGh(None),
        )
        .unwrap();
        assert_eq!(token.expose(), "github");
    }

    #[test]
    fn no_token_anywhere_names_gh_auth_login_and_the_app_variable() {
        let error = resolve(
            &source(),
            None,
            &env(&[]),
            &no_sign_in,
            &FakeGh(Some(" \n")),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("gh auth login"), "{message}");
        assert!(message.contains("APP_GITHUB_TOKEN"), "{message}");
        assert!(message.contains("owner/name"), "{message}");
    }

    #[test]
    fn a_signed_in_account_comes_after_the_variables_and_before_gh() {
        let signed_in = |_: Option<&str>| Some(Secret::new("signed-in"));
        let gh = FakeGh(Some("from-gh"));
        let token = resolve(&source(), None, &env(&[]), &signed_in, &gh).unwrap();
        assert_eq!(token.expose(), "signed-in");
        let token = resolve(
            &source(),
            None,
            &env(&[("GH_TOKEN", "gh")]),
            &signed_in,
            &gh,
        )
        .unwrap();
        assert_eq!(token.expose(), "gh");
    }

    #[test]
    fn a_selected_gh_account_wins_over_an_unrelated_subspace_login() {
        struct Selected;
        impl Runner for Selected {
            fn run(
                &self,
                program: &str,
                args: &[&str],
                remove: &[&str],
                _: Duration,
            ) -> Result<String, String> {
                assert_eq!(program, "gh");
                assert_eq!(
                    args,
                    [
                        "auth",
                        "token",
                        "--hostname",
                        "github.com",
                        "--user",
                        "chosen"
                    ]
                );
                assert_eq!(remove, ["GH_TOKEN", "GITHUB_TOKEN"]);
                Ok("chosen-token".into())
            }
        }
        fn other_sign_in(login: Option<&str>) -> Option<Secret> {
            login.is_none().then(|| Secret::new("other"))
        }
        let token = resolve(
            &source(),
            Some("chosen"),
            &env(&[]),
            &other_sign_in,
            &Selected,
        )
        .unwrap();
        assert_eq!(token.expose(), "chosen-token");
    }

    #[test]
    fn a_missing_selected_token_never_uses_another_account() {
        struct OtherAccount;
        impl Runner for OtherAccount {
            fn run(
                &self,
                _: &str,
                args: &[&str],
                _: &[&str],
                _: Duration,
            ) -> Result<String, String> {
                if args.ends_with(&["--user", "chosen"]) {
                    Err("chosen has no token".into())
                } else {
                    Ok("different-account-token".into())
                }
            }
        }
        let signed_in = |login: Option<&str>| login.is_none().then(|| Secret::new("other"));
        let error = resolve(
            &source(),
            Some("chosen"),
            &env(&[]),
            &signed_in,
            &OtherAccount,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("chosen") && error.contains("gh auth login"),
            "{error}"
        );
        let explicit = resolve(
            &source(),
            Some("chosen"),
            &env(&[("GH_TOKEN", "explicit")]),
            &signed_in,
            &OtherAccount,
        )
        .unwrap();
        assert_eq!(explicit.expose(), "explicit");
    }

    #[test]
    fn the_debug_form_hides_the_value() {
        let token = Token::new("super-secret");
        assert_eq!(format!("{token:?}"), "Token(redacted)");
        assert!(!format!("{token:?}").contains("super-secret"));
    }
}
