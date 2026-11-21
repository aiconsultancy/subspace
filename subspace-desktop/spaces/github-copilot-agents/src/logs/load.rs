//! Reading one session log: what the Show log action starts, the cache file
//! it prefers, and the refresh the watcher's round asks for. Every read
//! blocks, so each one runs through `cx.background_spawn`.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::Client as GithubClient;
use subspace_framework::client_github::copilot_agent_sessions::{self, SessionLog};
use subspace_framework::core::http::Error;
use subspace_framework::core::task_state::is_active;
use subspace_framework::dashboard::{full_text, full_text_service, prefs};

use crate::CopilotAgents;
use crate::actions::say;
use crate::load::as_one_account;
use crate::logs::{self, LogState, OpenLog};

// qual:allow(coupling, oi) reason: "One file per concern of the Copilot Agents dashboard, as its list, detail and action files already are; this one owns reading a session log."
impl CopilotAgents {
    /// Shows the open task's last session log, or hides the one on show.
    /// Only this shows a log. A loaded task detail may also read its last
    /// log for the search index, and only while that index is on for this
    /// Space: see [`prefetch_latest`].
    pub(crate) fn toggle_log(&mut self, cx: &mut Context<Self>) {
        let open = self.log.as_ref().map(|log| log.task_key.clone());
        match open {
            Some(key) if Some(key.as_str()) == self.detail_key.as_deref() => self.close_log(cx),
            _ => self.open_log(cx),
        }
    }

    fn close_log(&mut self, cx: &mut Context<Self>) {
        self.log = None;
        cx.notify();
    }

    /// Starts the read for the session the task's result came from, which is
    /// the last one the detail holds.
    fn open_log(&mut self, cx: &mut Context<Self>) {
        if !logs::logs_on(cx) {
            say("Session logs are turned off", cx);
            return;
        }
        let Some(key) = self.detail_key.clone() else {
            say("Open a cloud task first", cx);
            return;
        };
        let Some(detail) = self.detail.as_ref() else {
            say(
                "The task is still loading. Show the log again in a moment",
                cx,
            );
            return;
        };
        let Some(session) = detail.sessions.last() else {
            say("This task has no session to read a log for", cx);
            return;
        };
        let cache_key = logs::cache_key(&session.id, &session.updated_at);
        self.log = Some(OpenLog {
            task_key: key,
            session_id: session.id.clone(),
            cache_key,
            state: LogState::Loading,
        });
        self.read_log(true, cx);
    }

    /// Reads the open log, from the cache file when it holds this very
    /// session, else from the Copilot API. A refresh of a running task
    /// neither reads nor writes the cache: its key cannot tell the two
    /// reads apart.
    fn read_log(&mut self, cache: bool, cx: &mut Context<Self>) {
        let (Some(open), Some(client)) = (self.log.clone(), self.client.clone()) else {
            return;
        };
        self.log_request = self.log_request.wrapping_add(1);
        let request = self.log_request;
        let key = cache.then(|| open.cache_key.clone());
        let search = full_text_service(cx);
        let links = self
            .detail
            .as_ref()
            .map(logs::TaskLinks::from_detail)
            .unwrap_or_else(|| logs::TaskLinks::from_task_key(&open.task_key));
        cx.spawn(async move |this, cx| {
            let (loaded, indexed) = cx
                .background_spawn(async move {
                    let loaded = fetch_bound(&client, &open.session_id, key.as_deref());
                    let indexed = loaded.as_ref().ok().and_then(|(log, _, login)| {
                        search.as_ref().and_then(|service| {
                            index_log(
                                service,
                                login,
                                LogIndex {
                                    task_key: &open.task_key,
                                    session_id: &open.session_id,
                                    links: &links,
                                    log,
                                },
                            )
                            .err()
                        })
                    });
                    (loaded, indexed)
                })
                .await;
            _ = this.update(cx, |this, cx| {
                if indexed.is_some() {
                    say("Session log search cache could not be updated", cx);
                }
                this.apply_log(request, loaded, cx)
            });
        })
        .detach();
        cx.notify();
    }

    /// Takes a log answer, unless a newer request replaced what it answers.
    /// A failed read says the log is unavailable rather than raising an
    /// error: the endpoint is undocumented and a session may hold nothing.
    /// An API answer is cached under the account it was read as.
    fn apply_log(&mut self, request: u64, loaded: Result<BoundLog, Error>, cx: &mut Context<Self>) {
        let Some(open) = self.log.as_mut().filter(|_| request == self.log_request) else {
            return;
        };
        match loaded {
            Ok((log, true, login)) => {
                let text = logs::encode(&logs::account_key(&login, &open.cache_key), &log);
                open.state = LogState::Ready(log);
                if let Some(text) = text {
                    prefs::write_cache(logs::CACHE_FILE, text, cx);
                }
            }
            Ok((log, false, _)) => open.state = LogState::Ready(log),
            Err(_) => open.state = LogState::Unavailable,
        }
        cx.notify();
    }

    /// The watcher's round: an open log of a running task is read again, so
    /// a task that is still working shows what it did since. Nothing is
    /// streamed.
    pub(crate) fn reload_open_log(&mut self, cx: &mut Context<Self>) {
        let active = self
            .log
            .as_ref()
            .and_then(|log| self.row(&log.task_key))
            .is_some_and(|row| is_active(&row.state));
        if active {
            self.read_log(false, cx);
        }
    }
}

/// Indexes a log under the search scope of `login`, the work account it
/// was read as (see [`fetch_bound`]), never under whichever account holds
/// the credential once the read is over.
fn index_log(
    service: &full_text::Service,
    login: &str,
    request: LogIndex<'_>,
) -> Result<(), String> {
    if !indexes_logs(service)? {
        return Ok(());
    }
    let scope = logs::scope(service.app_id(), login).ok_or("Missing GitHub identity")?;
    let ticket = service.begin_activation("github-copilot-agents")?;
    let activation = service.prepare_activation_ticket(&ticket, &scope)?;
    service.activate_prepared(&activation)?;
    let lease = service.lease(&scope)?;
    let document = logs::document(
        request.task_key,
        request.session_id,
        request.log,
        chrono::Utc::now().timestamp(),
    )
    .ok_or("Session log could not be serialized")?;
    let id = document.id.clone();
    service.upsert_leased(&lease, &[document])?;
    service.index_metadata(&scope, &id, &logs::metadata(request.links))
}

struct LogIndex<'a> {
    task_key: &'a str,
    session_id: &'a str,
    links: &'a logs::TaskLinks,
    log: &'a SessionLog,
}

/// Whether the search index takes this Space's session logs.
fn indexes_logs(service: &full_text::Service) -> Result<bool, String> {
    Ok(service.policy("github-copilot-agents")?.enabled)
}

/// Reads a loaded task's last session log for the search index, without
/// showing it. Nothing is read unless session logs are on and the search
/// index takes this Space's logs, so selecting a task reads no log the
/// index would not keep.
pub(crate) fn prefetch_latest(
    client: Option<Arc<GithubClient>>,
    task_key: String,
    detail: &subspace_framework::client_github::copilot_agents::TaskDetail,
    cx: &mut Context<CopilotAgents>,
) {
    if !logs::logs_on(cx) {
        return;
    }
    let (Some(client), Some(session), Some(service)) = (
        client,
        detail.sessions.last().cloned(),
        full_text_service(cx),
    ) else {
        return;
    };
    let links = logs::TaskLinks::from_detail(detail);
    cx.spawn(async move |_, cx| {
        let _ = cx
            .background_spawn(async move {
                let key = logs::cache_key(&session.id, &session.updated_at);
                prefetch(
                    || indexes_logs(&service),
                    || {
                        fetch_bound(&client, &session.id, Some(&key))
                            .map(|(log, _, login)| (log, login))
                            .map_err(|error| error.to_string())
                    },
                    |(log, login)| {
                        index_log(
                            &service,
                            login,
                            LogIndex {
                                task_key: &task_key,
                                session_id: &session.id,
                                links: &links,
                                log,
                            },
                        )
                    },
                )
            })
            .await;
    })
    .detach();
}

/// The prefetch's order: the index policy first, and the log only when the
/// index will keep it.
fn prefetch<T>(
    indexed: impl FnOnce() -> Result<bool, String>,
    fetch: impl FnOnce() -> Result<T, String>,
    index: impl FnOnce(&T) -> Result<(), String>,
) -> Result<(), String> {
    if !indexed()? {
        return Ok(());
    }
    index(&fetch()?)
}

/// A log, whether it came from the API and is worth caching, and the work
/// account it was read as.
type BoundLog = (SessionLog, bool, String);

/// One log read bound to one work account ([`as_one_account`]): the cache
/// lookup, the API read and the account the answer is cached and indexed
/// under are the same, and a read during which the account changed is
/// refused.
fn fetch_bound(
    client: &GithubClient,
    session_id: &str,
    cache_key: Option<&str>,
) -> Result<BoundLog, Error> {
    as_one_account(
        || client.work_login(),
        |login| fetch_log(client, login, session_id, cache_key),
    )
    .map(|((log, fresh), login)| (log, fresh, login))
}

/// One log read: the cache file when `cache_key` is given and holds this
/// session as read by `login`, else the Copilot API. The flag says the
/// answer came from the API and is worth caching.
fn fetch_log(
    client: &GithubClient,
    login: &str,
    session_id: &str,
    cache_key: Option<&str>,
) -> Result<(SessionLog, bool), Error> {
    let cached = cache_key.and_then(|key| logs::cached(&logs::account_key(login, key)));
    if let Some(log) = cached {
        return Ok((log, false));
    }
    let log =
        client.copilot_agent_session_log(session_id, copilot_agent_sessions::MAX_LOG_BYTES)?;
    Ok((log, cache_key.is_some()))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use subspace_framework::client_github::copilot_agent_sessions::SessionLog;
    use subspace_framework::core::http::Error;

    use super::prefetch;
    use crate::load::{ACCOUNT_CHANGED, as_one_account};

    /// The work login on each lookup: `first`, then `then` from the second
    /// lookup on, as a credential that changes during a read.
    fn work_login<'a>(first: &'a str, then: &'a str) -> impl Fn() -> Result<String, Error> + 'a {
        let lookups = Cell::new(0);
        move || {
            let at = lookups.get();
            lookups.set(at + 1);
            Ok(if at == 0 { first } else { then }.to_owned())
        }
    }

    /// A prefetch whose log read is bound to the work account, and the
    /// account the log was indexed under, if it was.
    fn bound_prefetch(first: &str, then: &str) -> (Result<(), String>, Option<String>) {
        let indexed_as = RefCell::new(None);
        let result = prefetch(
            || Ok(true),
            || {
                as_one_account(work_login(first, then), |login| {
                    assert_eq!(
                        login, first,
                        "the read and its cache lookup use that account"
                    );
                    Ok(empty_log())
                })
                .map_err(|error| error.to_string())
            },
            |(_, login): &(SessionLog, String)| {
                *indexed_as.borrow_mut() = Some(login.clone());
                Ok(())
            },
        );
        (result, indexed_as.into_inner())
    }

    #[test]
    fn a_log_is_indexed_under_the_account_it_was_read_as() {
        let (result, indexed_as) = bound_prefetch("octo-dev", "octo-dev");
        assert_eq!(result, Ok(()));
        assert_eq!(indexed_as.as_deref(), Some("octo-dev"));
    }

    #[test]
    fn a_log_read_while_the_work_account_changed_is_not_indexed() {
        let (result, indexed_as) = bound_prefetch("octo-dev", "octo-other");
        assert_eq!(result, Err(ACCOUNT_CHANGED.to_owned()));
        assert_eq!(indexed_as, None, "nothing reaches either account's search");
    }

    fn empty_log() -> SessionLog {
        SessionLog {
            entries: Vec::new(),
            truncated: false,
        }
    }

    #[test]
    fn a_prefetch_reads_no_log_the_search_index_would_not_keep() {
        let fetched = Cell::new(false);
        let indexed = Cell::new(false);
        let skipped = prefetch(
            || Ok(false),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_| {
                indexed.set(true);
                Ok(())
            },
        );
        assert_eq!(skipped, Ok(()));
        assert!(!fetched.get(), "the index is off, so no log is read");
        assert!(!indexed.get());

        let failed = prefetch(
            || Err("no index".into()),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_| Ok(()),
        );
        assert_eq!(failed, Err("no index".to_owned()));
        assert!(!fetched.get(), "an unreadable policy reads no log either");

        let kept = prefetch(
            || Ok(true),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_| {
                indexed.set(true);
                Ok(())
            },
        );
        assert_eq!(kept, Ok(()));
        assert!(
            fetched.get() && indexed.get(),
            "an index that keeps it reads it"
        );
    }
}
