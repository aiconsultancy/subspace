//! The github.com accounts Subspace can use: the accounts signed in through
//! Subspace and gh's accounts, one entry per login.

use super::signed_in::SignedInAccount;

/// The github.com accounts: `logins` holds each login once, compared without
/// case, with the signed-in accounts first in GitHub's spelling. `active` is
/// gh's active account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubAccounts {
    pub logins: Vec<String>,
    pub active: Option<String>,
    pub signed_in: Vec<SignedInAccount>,
}

/// Where an account's token comes from, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountSource {
    /// Signed in through Subspace.
    SignedIn,
    /// Signed in through Subspace, but GitHub rejected the token.
    SignInExpired,
    /// The gh CLI's account.
    GitHubCli,
}

impl AccountSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::SignedIn => "Signed in",
            Self::SignInExpired => "Sign-in expired",
            Self::GitHubCli => "GitHub CLI",
        }
    }
}

impl GitHubAccounts {
    /// The saved account when it is still known, else gh's active account,
    /// else the first one, in the list's spelling.
    pub fn pick(&self, saved: Option<&str>) -> Option<String> {
        let known = |login: &str| {
            self.logins
                .iter()
                .find(|known| known.eq_ignore_ascii_case(login))
                .cloned()
        };
        saved
            .and_then(known)
            .or_else(|| self.active.as_deref().and_then(known))
            .or_else(|| self.active.clone())
            .or_else(|| self.logins.first().cloned())
    }

    /// Where `login`'s token comes from. A signed-in account comes first,
    /// as its token does.
    pub fn source(&self, login: &str) -> AccountSource {
        match self
            .signed_in
            .iter()
            .find(|account| account.login.eq_ignore_ascii_case(login))
        {
            Some(account) if account.expired => AccountSource::SignInExpired,
            Some(_) => AccountSource::SignedIn,
            None => AccountSource::GitHubCli,
        }
    }

    /// gh's accounts merged with the signed-in ones; `None` without any.
    pub(super) fn merge(gh: Option<Self>, signed_in: Vec<SignedInAccount>) -> Option<Self> {
        let gh = gh.unwrap_or_default();
        let mut logins: Vec<String> = signed_in
            .iter()
            .map(|account| account.login.clone())
            .collect();
        for login in gh.logins {
            if !logins
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&login))
            {
                logins.push(login);
            }
        }
        let active = gh.active.map(|active| {
            logins
                .iter()
                .find(|login| login.eq_ignore_ascii_case(&active))
                .cloned()
                .unwrap_or(active)
        });
        (!logins.is_empty()).then_some(Self {
            logins,
            active,
            signed_in,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountSource, GitHubAccounts, SignedInAccount};

    fn signed_in(login: &str, expired: bool) -> SignedInAccount {
        SignedInAccount {
            login: login.into(),
            expired,
        }
    }

    #[test]
    fn the_saved_account_wins_while_it_is_still_known() {
        let accounts = GitHubAccounts {
            logins: vec!["Work_corp".into(), "personal".into()],
            active: Some("personal".into()),
            signed_in: Vec::new(),
        };
        assert_eq!(
            accounts.pick(Some("work_CORP")).as_deref(),
            Some("Work_corp")
        );
        assert_eq!(accounts.pick(Some("gone")).as_deref(), Some("personal"));
        assert_eq!(accounts.pick(None).as_deref(), Some("personal"));
        let no_active = GitHubAccounts {
            logins: vec!["first".into(), "second".into()],
            ..GitHubAccounts::default()
        };
        assert_eq!(no_active.pick(None).as_deref(), Some("first"));
        assert_eq!(GitHubAccounts::default().pick(Some("anyone")), None);
    }

    #[test]
    fn signed_in_and_gh_accounts_merge_once_in_github_s_spelling() {
        let gh = GitHubAccounts {
            logins: vec!["octo-DEV".into(), "mona".into()],
            active: Some("octo-dev".into()),
            signed_in: Vec::new(),
        };
        let merged = GitHubAccounts::merge(
            Some(gh),
            vec![signed_in("Octo-Dev", false), signed_in("hubot", true)],
        )
        .unwrap();
        assert_eq!(merged.logins, ["Octo-Dev", "hubot", "mona"]);
        assert_eq!(merged.active.as_deref(), Some("Octo-Dev"));
        assert_eq!(merged.source("octo-dev"), AccountSource::SignedIn);
        assert_eq!(merged.source("HUBOT"), AccountSource::SignInExpired);
        assert_eq!(merged.source("mona"), AccountSource::GitHubCli);
        assert_eq!(AccountSource::GitHubCli.label(), "GitHub CLI");
    }

    #[test]
    fn without_gh_the_signed_in_accounts_are_the_accounts() {
        let merged = GitHubAccounts::merge(None, vec![signed_in("octo-dev", false)]).unwrap();
        assert_eq!(merged.logins, ["octo-dev"]);
        assert_eq!(merged.active, None);
        assert_eq!(merged.pick(None).as_deref(), Some("octo-dev"));
        assert_eq!(GitHubAccounts::merge(None, Vec::new()), None);
    }
}
