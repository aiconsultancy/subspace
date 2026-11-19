//! Account discovery and background loads. Late account results are discarded.
//! A full load reads the repositories one after another and shows each as it
//! arrives, so a slow repository does not hold back the others.

mod fetch;
pub(crate) mod mapping;

use std::time::Instant;

use gpui_kit::{App, AppContext as _, Context};
use subspace_framework::client_github::lifecycle::REFRESH_INTERVAL;
use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_framework::dashboard::{Dashboard as _, prefs, refresh};

use crate::{
    Actions,
    fetch::Fetch,
    model::{Record, Snapshot},
};

pub(crate) struct RepoResult {
    repo: String,
    runs: Result<Vec<Record>, String>,
    deployments: Result<Vec<Record>, String>,
    releases: Result<Vec<Record>, String>,
}

/// The account picked on this board, else the shared GitHub account.
fn saved_account(cx: &App) -> Option<String> {
    prefs::get(cx, "actions.account").or_else(|| prefs::get(cx, GITHUB_ACCOUNT_KEY))
}

impl Actions {
    pub(crate) fn full_load_due(&self, cx: &App) -> bool {
        let interval = refresh::interval(Self::ID, REFRESH_INTERVAL, cx);
        self.last_full_load_started_at
            .is_none_or(|at| at.elapsed() >= interval)
    }

    pub(crate) fn refresh_all(&mut self, cx: &mut Context<Self>) {
        if self.login.is_none() {
            self.discover(cx);
        } else {
            self.load_all(cx);
        }
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        if !self.fetch.is_idle() || self.repos.is_empty() {
            self.rebuild(cx);
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.last_full_load_started_at = Some(Instant::now());
        let generation = self.generation;
        self.fetch = Fetch::discovering(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { client.accounts() }).await;
            _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.fetch.finish();
                match result {
                    Ok(accounts) => {
                        this.login = accounts.pick(saved_account(cx).as_deref());
                        this.accounts = accounts.logins;
                        this.load_repos(this.repos.clone(), cx);
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                        this.rebuild(cx);
                    }
                }
            });
        }));
        self.rebuild(cx);
    }

    /// The favourite repositories changed without a restart: a star in the
    /// PRs board or a Settings Save. Shown, the board loads them now.
    pub(crate) fn follow_repos(&mut self, cx: &mut Context<Self>) {
        let repos = subspace_framework::dashboard::repositories::favourites(cx);
        if repos == self.repos {
            return;
        }
        self.repos = repos;
        self.loaded_at = None;
        self.last_full_load_started_at = None;
        self.queued_repos.retain(|repo| self.repos.contains(repo));
        match (self.visible, self.login.is_some()) {
            (true, true) => self.load_all(cx),
            (true, false) => self.discover(cx),
            (false, _) => self.rebuild(cx),
        }
    }

    pub(crate) fn load_all(&mut self, cx: &mut Context<Self>) {
        let started_at = Instant::now();
        if self.load_repos(self.repos.clone(), cx) {
            self.last_full_load_started_at = Some(started_at);
        }
    }

    /// A repository refresh coalesces with a load that already covers it.
    /// Otherwise it waits for the active fetch to finish.
    pub(crate) fn refresh_repo(&mut self, repo: String, cx: &mut Context<Self>) {
        if self.login.is_none() || !self.repos.contains(&repo) || self.fetch.loads_repo(&repo) {
            return;
        }
        self.queued_repos.insert(repo);
        self.load_queued_repos(cx);
    }

    pub(crate) fn load_queued_repos(&mut self, cx: &mut Context<Self>) {
        if self.fetch.is_idle() {
            let repos = self.queued_repos.iter().cloned().collect();
            self.load_repos(repos, cx);
        }
    }

    fn load_repos(&mut self, repos: Vec<String>, cx: &mut Context<Self>) -> bool {
        if !self.fetch.is_idle() || repos.is_empty() {
            return false;
        }
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return false;
        };
        self.queued_repos.retain(|repo| !repos.contains(repo));
        let pending = repos.clone();
        let generation = self.generation;
        let task = cx.spawn(async move |this, cx| {
            for repo in &repos {
                let repo = repo.clone();
                let (client, login) = (client.clone(), login.clone());
                let result = cx
                    .background_spawn(async move { fetch::repository(&client, &login, &repo) })
                    .await;
                if this
                    .update(cx, |this, cx| this.apply_repo(generation, result, cx))
                    .is_err()
                {
                    return;
                }
            }
            _ = this.update(cx, |this, cx| {
                this.apply_results(generation, &repos, Vec::new(), cx)
            });
        });
        self.fetch = Fetch::loading(task, &pending);
        self.rebuild(cx);
        true
    }

    /// One repository of a running full load arrived: show it now.
    pub(crate) fn apply_repo(
        &mut self,
        generation: u64,
        result: RepoResult,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        self.fetch.delivered(&result.repo);
        self.merge_results(vec![result]);
        self.rebuild(cx);
    }

    /// A repository load ended, with any results not yet applied.
    pub(crate) fn apply_results(
        &mut self,
        generation: u64,
        covered: &[String],
        results: Vec<RepoResult>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        let follow_up = self.fetch.finish();
        if !self.merge_results(results) {
            self.rebuild(cx);
            return;
        }
        self.record_full_refresh(covered);
        self.rebuild(cx);
        if follow_up.is_empty() {
            self.load_queued_repos(cx);
        } else {
            crate::poll::refresh_runs(self, follow_up, cx);
        }
    }

    fn record_full_refresh(&mut self, covered: &[String]) {
        if covered.len() != self.repos.len()
            || !self.repos.iter().all(|repo| covered.contains(repo))
        {
            return;
        }
        if self.error.is_none() {
            self.loaded_at = Some(Instant::now());
            self.backoff.succeeded();
        } else {
            self.backoff.failed();
        }
    }
}

impl Actions {
    /// Merges `results` into the current account's records and restates the
    /// error banner. False when no account is chosen.
    fn merge_results(&mut self, results: Vec<RepoResult>) -> bool {
        let Some(login) = &self.login else {
            return false;
        };
        let snapshot = self.data.entry(login.to_lowercase()).or_default();
        for result in results {
            merge(snapshot, result);
        }
        let errors = snapshot.errors.values().cloned().collect::<Vec<_>>();
        self.error = (!errors.is_empty()).then(|| errors.join("\n"));
        true
    }
}

fn merge(snapshot: &mut Snapshot, result: RepoResult) {
    let RepoResult {
        repo,
        runs,
        deployments,
        releases,
    } = result;
    let outcomes = [
        ("Runs", replace_records(&mut snapshot.runs, &repo, runs)),
        (
            "Deployments",
            replace_records(&mut snapshot.deployments, &repo, deployments),
        ),
        (
            "Releases",
            replace_records(&mut snapshot.releases, &repo, releases),
        ),
    ];
    for (source, outcome) in outcomes {
        record_outcome(snapshot, &format!("{repo}: {source}"), outcome);
    }
}

/// Replaces `repo`'s records with a successful read; a failed read keeps them.
fn replace_records(
    records: &mut Vec<Record>,
    repo: &str,
    result: Result<Vec<Record>, String>,
) -> Result<(), String> {
    let mut fresh = result?;
    records.retain(|row| row.repo != repo);
    records.append(&mut fresh);
    Ok(())
}

/// Notes a source read as `key` (`owner/name: Source`): read, or failed.
fn record_outcome(snapshot: &mut Snapshot, key: &str, outcome: Result<(), String>) {
    match outcome {
        Ok(()) => {
            snapshot.errors.remove(key);
            snapshot.loaded.insert(key.to_owned());
        }
        Err(error) => {
            snapshot
                .errors
                .insert(key.to_owned(), format!("{key}: {error}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RepoResult, merge, saved_account};
    use crate::model::Snapshot;
    use gpui_kit::TestAppContext;
    use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY;
    use subspace_framework::dashboard::prefs;

    #[gpui_kit::test]
    fn saved_account_prefers_the_board_choice_then_the_shared_account(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert_eq!(saved_account(cx), None);
            prefs::set(cx, GITHUB_ACCOUNT_KEY, "octo-dev");
            assert_eq!(saved_account(cx).as_deref(), Some("octo-dev"));
            prefs::set(cx, "actions.account", "octo-dev_acme");
            assert_eq!(saved_account(cx).as_deref(), Some("octo-dev_acme"));
        });
    }

    #[test]
    fn failed_source_is_named_without_discarding_other_source_errors() {
        let mut snapshot = Snapshot::default();
        merge(
            &mut snapshot,
            RepoResult {
                repo: "a/b".into(),
                runs: Err("denied".into()),
                deployments: Ok(Vec::new()),
                releases: Err("rate limited".into()),
            },
        );
        assert_eq!(snapshot.errors.len(), 2);
        assert_eq!(snapshot.errors["a/b: Runs"], "a/b: Runs: denied");
        assert_eq!(
            snapshot.loaded.iter().collect::<Vec<_>>(),
            ["a/b: Deployments"]
        );
        merge(
            &mut snapshot,
            RepoResult {
                repo: "a/b".into(),
                runs: Ok(Vec::new()),
                deployments: Err("gone".into()),
                releases: Ok(Vec::new()),
            },
        );
        assert_eq!(snapshot.errors.len(), 1, "a later success clears its error");
        assert!(
            snapshot.loaded.contains("a/b: Deployments"),
            "a later failure keeps what was read"
        );
        assert_eq!(snapshot.loaded.len(), 3);
    }
}

#[cfg(test)]
mod stream_tests;
