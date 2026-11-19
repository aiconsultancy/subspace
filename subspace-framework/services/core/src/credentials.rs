//! Credential sources and precedence (REQ-21 to REQ-24, REQ-31, D-3).
//!
//! | Service | 1. Environment | 2. Config file | 3. Signed in | 4. CLI |
//! | --- | --- | --- | --- | --- |
//! | GitHub | `GH_TOKEN`, `GITHUB_TOKEN` | `github.token` | Sign in with GitHub, per account | `gh auth token` per account |
//!
//! An environment variable that is set but empty counts as unset. The
//! environment is read on each call, so a retry after HTTP 401 (REQ-22) sees
//! a changed variable. A signed-in or `gh` token is cached in memory until
//! [`Credentials::invalidate`]; a rejected signed-in token also marks its
//! account as needing sign-in again.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::Secret;
use crate::config::{self, Config};
use crate::state::{self, State};

mod accounts;
mod credential;
pub mod gh_sign_in;
mod hosts_yml;
mod runner;
pub mod signed_in;

pub use accounts::{AccountSource, GitHubAccounts};
pub use credential::{Credential, Scheme, Source};
pub use hosts_yml::is_login;
pub use runner::{Runner, SystemRunner};
pub use signed_in::{SignedInAccount, SignedInAccounts, StoreError};

use hosts_yml::parse_hosts_yml;

const GH_TIMEOUT: Duration = Duration::from_secs(15);

/// No credential for a service, with the guidance that REQ-22 shows. The
/// message never contains a credential or CLI output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CredentialError(pub String);

/// The process environment, injectable so tests never read the real one.
pub trait Environment: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
}

pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// The state key of the account the GitHub dashboard and Copilot chat use.
pub const GITHUB_ACCOUNT_KEY: &str = "github.activeAccount";

/// The account saved under [`GITHUB_ACCOUNT_KEY`] in the state file.
pub fn saved_account() -> Option<String> {
    let saved = State::load(&state::path()?);
    saved.get(GITHUB_ACCOUNT_KEY).map(str::to_owned)
}

/// The credential sources of every service.
pub struct Credentials {
    environment: Box<dyn Environment>,
    runner: Box<dyn Runner>,
    config: Config,
    signed_in: SignedInAccounts,
    /// Whether the app offers Sign in with GitHub, so guidance may name it.
    sign_in_offered: bool,
    tokens: Mutex<HashMap<String, Secret>>,
    gh: Mutex<HashMap<String, Secret>>,
}

impl Credentials {
    /// Uses the process environment, the signed-in accounts and the real
    /// `gh` CLI.
    pub fn from_system(config: &Config) -> Self {
        Self::new(config, ProcessEnvironment, SystemRunner)
            .with_signed_in(SignedInAccounts::system())
    }

    /// Signed-in accounts are kept in memory until [`Self::with_signed_in`].
    pub fn new(
        config: &Config,
        environment: impl Environment + 'static,
        runner: impl Runner + 'static,
    ) -> Self {
        Self {
            environment: Box::new(environment),
            runner: Box::new(runner),
            config: config.clone(),
            signed_in: SignedInAccounts::memory(),
            sign_in_offered: false,
            tokens: Mutex::new(HashMap::new()),
            gh: Mutex::new(HashMap::new()),
        }
    }

    /// Reads and keeps signed-in accounts in `accounts`.
    pub fn with_signed_in(mut self, accounts: SignedInAccounts) -> Self {
        self.signed_in = accounts;
        self
    }

    /// Whether the app offers Sign in with GitHub (it has an OAuth App).
    /// Guidance names it only then; the credential order is the same.
    pub fn with_sign_in_offered(mut self, offered: bool) -> Self {
        self.sign_in_offered = offered;
        self
    }

    pub fn sign_in_offered(&self) -> bool {
        self.sign_in_offered
    }

    /// A set, non-empty environment variable, trimmed.
    fn var(&self, name: &str) -> Option<String> {
        let value = self.environment.var(name)?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    }

    /// The first environment variable in `names` that is set, else the
    /// config file token.
    fn env_or_config(
        &self,
        names: &[&'static str],
        config: &Option<Secret>,
        key: &'static str,
        scheme: Scheme,
    ) -> Option<Credential> {
        let from_env = names.iter().find_map(|&name| {
            self.var(name)
                .map(|value| Credential::new(Secret::new(value), scheme, Source::Environment(name)))
        });
        from_env.or_else(|| {
            config
                .clone()
                .map(|value| Credential::new(value, scheme, Source::ConfigFile(key)))
        })
    }

    /// The GitHub credential. Without an environment or config token, the
    /// token of `login`'s account, or of gh's active account (else the first
    /// account) when `login` is `None`.
    pub fn github(&self, login: Option<&str>) -> Result<Credential, CredentialError> {
        let token = &self.config.github.token;
        if let Some(credential) = self.env_or_config(
            &["GH_TOKEN", "GITHUB_TOKEN"],
            token,
            "github.token",
            Scheme::Bearer,
        ) {
            return Ok(credential);
        }
        let login = match login {
            Some(login) => login.to_owned(),
            None => {
                let accounts = self.github_accounts()?;
                accounts
                    .active
                    .or_else(|| accounts.logins.into_iter().next())
                    .ok_or_else(|| no_accounts(self.sign_in_offered))?
            }
        };
        self.account(&login)
    }

    /// The token of exactly the account `login`: its sign-in through
    /// Subspace, else its gh CLI token. `GH_TOKEN`, `GITHUB_TOKEN` and the
    /// config file token are ignored.
    pub fn account(&self, login: &str) -> Result<Credential, CredentialError> {
        if let Some(credential) = self.signed_in_credential(login)? {
            return Ok(credential);
        }
        self.gh_cli(login)
            .map_err(|error| self.expired_guidance(login).unwrap_or(error))
    }

    /// The signed-in token of `login`, cached until it is invalidated.
    fn signed_in_credential(&self, login: &str) -> Result<Option<Credential>, CredentialError> {
        let Some(account) = self
            .signed_in
            .find(login)
            .filter(|account| !account.expired)
        else {
            return Ok(None);
        };
        let key = account.login.to_ascii_lowercase();
        let mut cached = lock(&self.tokens);
        let value = match cached.get(&key) {
            Some(value) => Some(value.clone()),
            None => self
                .signed_in
                .token(&account.login)
                .map_err(|error| CredentialError(error.to_string()))?,
        };
        Ok(value.map(|value| {
            cached.insert(key, value.clone());
            Credential::new(
                value,
                Scheme::Bearer,
                Source::SignedIn {
                    login: account.login,
                },
            )
        }))
    }

    /// What to say when `login` signed in through Subspace and GitHub has
    /// since rejected its token.
    fn expired_guidance(&self, login: &str) -> Option<CredentialError> {
        let account = self
            .signed_in
            .find(login)
            .filter(|account| account.expired)?;
        let next = if self.sign_in_offered {
            "Sign in with GitHub again in Settings."
        } else {
            "Set GH_TOKEN, or run `gh auth login`."
        };
        Some(CredentialError(format!(
            "GitHub no longer accepts the sign-in for {}. {next}",
            account.login
        )))
    }

    /// Keeps a new sign-in of `login`, replacing an earlier one.
    pub fn sign_in(&self, login: &str, token: &Secret) -> Result<(), CredentialError> {
        lock(&self.tokens).remove(&login.to_ascii_lowercase());
        self.signed_in
            .save(login, token)
            .map_err(|error| CredentialError(error.to_string()))
    }

    /// Forgets the sign-in of `login` and gives back its token, so it can be
    /// revoked. gh's own account of the same login is not changed.
    pub fn sign_out(&self, login: &str) -> Result<Option<Secret>, CredentialError> {
        lock(&self.tokens).remove(&login.to_ascii_lowercase());
        self.signed_in
            .remove(login)
            .map_err(|error| CredentialError(error.to_string()))
    }

    /// The gh CLI token of exactly `login`, ignoring `GH_TOKEN`, `GITHUB_TOKEN`
    /// and the config file token.
    pub fn gh_cli(&self, login: &str) -> Result<Credential, CredentialError> {
        let login = login.to_owned();
        let sources = token_sources(self.sign_in_offered, "the config file");
        let failed = || {
            CredentialError(format!(
                "Could not get a GitHub token for {login}. {sources}"
            ))
        };
        if !is_login(&login) {
            return Err(failed());
        }
        let mut cached = lock(&self.gh);
        let value = match cached.get(&login) {
            Some(value) => value.clone(),
            None => {
                // gh prefers GH_TOKEN and GITHUB_TOKEN over the account; drop them so --user decides.
                let args = [
                    "auth",
                    "token",
                    "--hostname",
                    "github.com",
                    "--user",
                    login.as_str(),
                ];
                let output = self
                    .runner
                    .run("gh", &args, &["GH_TOKEN", "GITHUB_TOKEN"], GH_TIMEOUT)
                    .map_err(|_| failed())?;
                let value = Some(output.trim())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(failed)?;
                let value = Secret::new(value);
                cached.insert(login.clone(), value.clone());
                value
            }
        };
        Ok(Credential {
            value,
            scheme: Scheme::Bearer,
            source: Source::GhCli { login },
        })
    }

    /// The github.com accounts: those signed in through Subspace and gh's,
    /// from `hosts.yml` in gh's config directory. Only logins are read,
    /// never token values.
    pub fn github_accounts(&self) -> Result<GitHubAccounts, CredentialError> {
        GitHubAccounts::merge(self.gh_accounts(), self.signed_in.accounts())
            .ok_or_else(|| no_accounts(self.sign_in_offered))
    }

    /// gh's github.com accounts, when gh has any.
    fn gh_accounts(&self) -> Option<GitHubAccounts> {
        let file = self.gh_config_dir()?.join("hosts.yml");
        let text = fs::read_to_string(file).ok()?;
        let mut accounts = parse_hosts_yml(&text);
        if accounts.logins.is_empty() {
            accounts.logins.extend(accounts.active.clone());
        }
        Some(accounts)
    }

    /// gh's config directory, found as gh finds it.
    fn gh_config_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = self.var("GH_CONFIG_DIR") {
            return Some(dir.into());
        }
        if let Some(dir) = self.var("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(dir).join("gh"));
        }
        if cfg!(windows)
            && let Some(dir) = self.var("AppData")
        {
            return Some(PathBuf::from(dir).join("GitHub CLI"));
        }
        dirs::home_dir().map(|home| home.join(".config").join("gh"))
    }

    /// Drops a cached token after the service rejected it (REQ-22), so the
    /// next call asks again. A rejected signed-in token marks its account as
    /// needing sign-in again.
    pub fn invalidate(&self, credential: &Credential) {
        match &credential.source {
            Source::GhCli { login } => self.forget_gh_token(login),
            Source::SignedIn { login } => {
                lock(&self.tokens).remove(&login.to_ascii_lowercase());
                // Best effort: a locked store keeps the account as it was.
                let _ = self.signed_in.reject(login, &credential.value);
            }
            Source::Environment(_) | Source::ConfigFile(_) => {}
        }
    }

    /// Drops the cached gh token of `login`, such as after a new sign-in.
    pub fn forget_gh_token(&self, login: &str) {
        lock(&self.gh).remove(login);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn no_accounts(sign_in_offered: bool) -> CredentialError {
    CredentialError(format!(
        "No GitHub accounts found. {}",
        token_sources(sign_in_offered, &config::place())
    ))
}

/// Where a GitHub token can come from, naming Sign in with GitHub only when
/// the app offers it.
fn token_sources(sign_in_offered: bool, config: &str) -> String {
    let lead = if sign_in_offered {
        "Sign in with GitHub in Settings, set"
    } else {
        "Set"
    };
    format!("{lead} GH_TOKEN, set github.token in {config}, or run `gh auth login`.")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        Credential, CredentialError, Credentials, Environment, GitHubAccounts, Runner, Scheme,
        Source, parse_hosts_yml,
    };
    use crate::Secret;

    use crate::config::Config;

    struct Env(HashMap<&'static str, String>);

    impl Environment for Env {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    fn env(pairs: &[(&'static str, &str)]) -> Env {
        Env(pairs
            .iter()
            .map(|&(name, value)| (name, value.to_owned()))
            .collect())
    }

    /// A fake CLI: answers each call with the next queued result and records
    /// the calls.
    #[derive(Clone, Default)]
    struct Fake {
        answers: Arc<Mutex<Vec<Result<String, String>>>>,
        calls: Arc<Mutex<Vec<(String, Vec<String>, Vec<String>)>>>,
    }

    impl Fake {
        fn answering(answers: &[Result<&str, &str>]) -> Self {
            let fake = Self::default();
            *fake.answers.lock().unwrap() = answers
                .iter()
                .rev()
                .map(|answer| answer.map(Into::into).map_err(Into::into))
                .collect();
            fake
        }

        fn calls(&self) -> Vec<(String, Vec<String>, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Runner for Fake {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            remove: &[&str],
            _timeout: Duration,
        ) -> Result<String, String> {
            let strings = |items: &[&str]| items.iter().map(ToString::to_string).collect();
            self.calls
                .lock()
                .unwrap()
                .push((program.into(), strings(args), strings(remove)));
            self.answers
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err("no answer".into()))
        }
    }

    fn config_with_tokens() -> Config {
        let mut config = Config::default();
        config.github.token = Some(Secret::new("config-gh"));
        config
    }

    fn source(result: Result<Credential, CredentialError>) -> Source {
        result.unwrap().source
    }

    #[test]
    fn e01_environment_wins_over_config_and_cli() {
        let fake = Fake::default();
        let environment = env(&[("GITHUB_TOKEN", "env-github"), ("GH_TOKEN", "env-gh")]);
        let credentials = Credentials::new(&config_with_tokens(), environment, fake.clone());

        let github = credentials.github(None).unwrap();
        assert_eq!(github.source, Source::Environment("GH_TOKEN"));
        assert_eq!(github.scheme, Scheme::Bearer);
        assert_eq!(github.value.expose(), "env-gh");
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn e02_config_wins_over_cli_and_empty_variables_count_as_unset() {
        let fake = Fake::default();
        let environment = env(&[("GH_TOKEN", "  "), ("GITHUB_TOKEN", "")]);
        let credentials = Credentials::new(&config_with_tokens(), environment, fake.clone());

        assert_eq!(
            source(credentials.github(Some("octo-dev"))),
            Source::ConfigFile("github.token")
        );
        assert!(fake.calls().is_empty());
    }

    const HOSTS: &str = "\
github.com:
    users:
        octo-dev_corp:
            oauth_token: gho_secret_one
        octocat:
            oauth_token: gho_secret_two
        \"bad login!\":
    git_protocol: https
    oauth_token: gho_secret_two  # active token
    user: octo-dev_corp
ghe.example.com:
    users:
        other:
    user: other
";

    #[test]
    fn hosts_yml_gives_logins_and_the_active_login_but_no_tokens() {
        let accounts = parse_hosts_yml(HOSTS);
        assert_eq!(
            accounts,
            GitHubAccounts {
                logins: vec!["octo-dev_corp".into(), "octocat".into()],
                active: Some("octo-dev_corp".into()),
                signed_in: Vec::new(),
            }
        );
        assert!(!format!("{accounts:?}").contains("gho_"));
    }

    #[test]
    fn hosts_yml_with_only_the_active_user() {
        let accounts = parse_hosts_yml("github.com:\n  user: 'solo'\n  oauth_token: x\n");
        assert_eq!(accounts.logins, Vec::<String>::new());
        assert_eq!(accounts.active.as_deref(), Some("solo"));
    }

    #[test]
    fn github_uses_gh_per_account_without_the_token_variables() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hosts.yml"), HOSTS).unwrap();
        let fake = Fake::answering(&[Ok("gho_one\n"), Ok("gho_two\n"), Ok("gho_one_new\n")]);
        let environment = env(&[
            ("GH_CONFIG_DIR", dir.path().to_str().unwrap()),
            ("GITHUB_TOKEN", ""),
        ]);
        let credentials = Credentials::new(&Config::default(), environment, fake.clone());

        assert_eq!(credentials.github_accounts().unwrap().logins.len(), 2);
        let active = credentials.github(None).unwrap();
        assert_eq!(active.value.expose(), "gho_one");
        assert_eq!(
            active.source,
            Source::GhCli {
                login: "octo-dev_corp".into()
            }
        );
        assert_eq!(
            credentials.github(Some("octocat")).unwrap().value.expose(),
            "gho_two"
        );
        // Cached per login until invalidated.
        assert_eq!(credentials.github(None).unwrap().value.expose(), "gho_one");
        credentials.invalidate(&active);
        assert_eq!(
            credentials.github(None).unwrap().value.expose(),
            "gho_one_new"
        );

        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "gh");
        assert_eq!(
            calls[0].1,
            [
                "auth",
                "token",
                "--hostname",
                "github.com",
                "--user",
                "octo-dev_corp"
            ]
        );
        assert_eq!(calls[0].2, ["GH_TOKEN", "GITHUB_TOKEN"]);
    }

    #[test]
    fn github_without_accounts_or_with_a_bad_login_gives_guidance() {
        let dir = tempfile::tempdir().unwrap();
        let environment = env(&[("GH_CONFIG_DIR", dir.path().to_str().unwrap())]);
        let fake = Fake::default();
        let credentials = Credentials::new(&Config::default(), environment, fake.clone());
        assert!(
            credentials
                .github(None)
                .unwrap_err()
                .0
                .contains("gh auth login")
        );
        assert!(credentials.github(Some("--help")).is_err());
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn missing_credentials_name_their_sources() {
        let dir = tempfile::tempdir().unwrap();
        let environment = env(&[("GH_CONFIG_DIR", dir.path().to_str().unwrap())]);
        let credentials = Credentials::new(&Config::default(), environment, Fake::default());
        // E-03: the message names the config file's path.
        let place = crate::config::place();
        let message = credentials.github(None).unwrap_err().0;
        assert!(
            message.contains(&format!("github.token in {place}")),
            "{message}"
        );
        assert!(message.contains("GH_TOKEN"), "{message}");
    }

    #[test]
    fn guidance_names_sign_in_with_github_only_when_the_app_offers_it() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = |offered| {
            let environment = env(&[("GH_CONFIG_DIR", dir.path().to_str().unwrap())]);
            Credentials::new(&Config::default(), environment, Fake::default())
                .with_signed_in(signed_in("octo-dev", "old"))
                .with_sign_in_offered(offered)
        };
        for offered in [false, true] {
            let credentials = credentials(offered);
            assert_eq!(credentials.sign_in_offered(), offered);
            let missing = credentials.gh_cli("octocat").unwrap_err().0;
            let rejected = credentials.github(Some("octo-dev")).unwrap();
            credentials.invalidate(&rejected);
            let expired = credentials.github(Some("octo-dev")).unwrap_err().0;
            for message in [missing, expired] {
                assert_eq!(
                    message.contains("Sign in with GitHub"),
                    offered,
                    "{message}"
                );
                assert!(offered || message.contains("gh auth login"), "{message}");
            }
        }
        let empty = Credentials::new(
            &Config::default(),
            env(&[("GH_CONFIG_DIR", dir.path().to_str().unwrap())]),
            Fake::default(),
        );
        let message = empty.github(None).unwrap_err().0;
        assert!(
            message.starts_with("No GitHub accounts found. Set GH_TOKEN"),
            "{message}"
        );
    }

    #[test]
    fn debug_and_errors_show_no_credential() {
        let credential = Credential {
            value: Secret::new("pat-value"),
            scheme: Scheme::Bearer,
            source: Source::Environment("GH_TOKEN"),
        };
        assert!(!format!("{credential:?}").contains("pat-value"));
        let header = credential.authorization();
        assert_eq!(format!("{header:?}"), "***");
        assert_eq!(header.expose(), "Bearer pat-value");
    }

    fn signed_in(login: &str, token: &str) -> crate::credentials::SignedInAccounts {
        let accounts = crate::credentials::SignedInAccounts::memory();
        accounts.save(login, &Secret::new(token)).unwrap();
        accounts
    }

    #[test]
    fn precedence_is_environment_config_signed_in_then_gh() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hosts.yml"), HOSTS).unwrap();
        let gh_dir = dir.path().to_str().unwrap();
        let with = |pairs: &[(&'static str, &str)], config: &Config| {
            Credentials::new(config, env(pairs), Fake::answering(&[Ok("gho_cli")]))
                .with_signed_in(signed_in("octocat", "signed-in-token"))
        };
        let environment = with(
            &[("GH_TOKEN", "env"), ("GH_CONFIG_DIR", gh_dir)],
            &config_with_tokens(),
        );
        assert_eq!(
            source(environment.github(Some("octocat"))),
            Source::Environment("GH_TOKEN")
        );
        let config = with(&[("GH_CONFIG_DIR", gh_dir)], &config_with_tokens());
        assert_eq!(
            source(config.github(Some("octocat"))),
            Source::ConfigFile("github.token")
        );
        let accounts = with(&[("GH_CONFIG_DIR", gh_dir)], &Config::default());
        let credential = accounts.github(Some("OCTOCAT")).unwrap();
        assert_eq!(credential.value.expose(), "signed-in-token");
        assert_eq!(
            credential.source,
            Source::SignedIn {
                login: "octocat".into()
            }
        );
        let gh = accounts.github(Some("octo-dev_corp")).unwrap();
        assert_eq!(gh.value.expose(), "gho_cli");
        assert!(matches!(gh.source, Source::GhCli { .. }));
    }

    #[test]
    fn signed_in_accounts_are_listed_with_gh_and_work_without_gh() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hosts.yml"), HOSTS).unwrap();
        let with_gh = Credentials::new(
            &Config::default(),
            env(&[("GH_CONFIG_DIR", dir.path().to_str().unwrap())]),
            Fake::default(),
        )
        .with_signed_in(signed_in("OctoCat", "token"));
        let accounts = with_gh.github_accounts().unwrap();
        assert_eq!(accounts.logins, ["OctoCat", "octo-dev_corp"]);
        let empty = tempfile::tempdir().unwrap();
        let without_gh = Credentials::new(
            &Config::default(),
            env(&[("GH_CONFIG_DIR", empty.path().to_str().unwrap())]),
            Fake::default(),
        )
        .with_signed_in(signed_in("octo-dev", "token"));
        assert_eq!(without_gh.github(None).unwrap().value.expose(), "token");
    }

    #[test]
    fn a_rejected_signed_in_token_asks_for_sign_in_again() {
        let empty = tempfile::tempdir().unwrap();
        let credentials = Credentials::new(
            &Config::default(),
            env(&[("GH_CONFIG_DIR", empty.path().to_str().unwrap())]),
            Fake::default(),
        )
        .with_signed_in(signed_in("octo-dev", "old"))
        .with_sign_in_offered(true);
        let rejected = credentials.github(Some("octo-dev")).unwrap();
        credentials.invalidate(&rejected);
        let message = credentials.github(Some("octo-dev")).unwrap_err().0;
        assert!(message.contains("Sign in with GitHub again"), "{message}");
        credentials
            .sign_in("octo-dev", &Secret::new("new"))
            .unwrap();
        assert_eq!(credentials.github(None).unwrap().value.expose(), "new");
        assert_eq!(
            credentials.sign_out("octo-dev").unwrap(),
            Some(Secret::new("new"))
        );
        assert!(credentials.github(None).is_err());
    }
}
