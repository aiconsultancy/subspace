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
use crate::logs::{self, LogState, OpenLog};

// qual:allow(coupling, oi) reason: "One file per concern of the Copilot Agents dashboard, as its list, detail and action files already are; this one owns reading a session log."
impl CopilotAgents {
    /// Shows the open task's last session log, or hides the one on show.
    /// The log is read only here, never on selection.
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
                    let loaded = fetch_log(&client, &open.session_id, key.as_deref());
                    let indexed = loaded.as_ref().ok().and_then(|(log, _)| {
                        search.as_ref().and_then(|service| {
                            index_log(
                                &client,
                                service,
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
    fn apply_log(
        &mut self,
        request: u64,
        loaded: Result<(SessionLog, bool), Error>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.log.as_mut().filter(|_| request == self.log_request) else {
            return;
        };
        match loaded {
            Ok((log, true)) => {
                let text = logs::encode(&open.cache_key, &log);
                open.state = LogState::Ready(log);
                if let Some(text) = text {
                    prefs::write_cache(logs::CACHE_FILE, text, cx);
                }
            }
            Ok((log, false)) => open.state = LogState::Ready(log),
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

fn index_log(
    client: &GithubClient,
    service: &full_text::Service,
    request: LogIndex<'_>,
) -> Result<(), String> {
    if !service.policy("github-copilot-agents")?.enabled {
        return Ok(());
    }
    let credential = client.credential(None).map_err(|error| error.to_string())?;
    let login = subspace_framework::client_github::credential_login(&credential)
        .map_err(|error| error.to_string())?;
    let scope = logs::scope(service.app_id(), &login).ok_or("Missing GitHub identity")?;
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
                let (log, _) = fetch_log(&client, &session.id, Some(&key))
                    .map_err(|error| error.to_string())?;
                index_log(
                    &client,
                    &service,
                    LogIndex {
                        task_key: &task_key,
                        session_id: &session.id,
                        links: &links,
                        log: &log,
                    },
                )
            })
            .await;
    })
    .detach();
}

/// One log read: the cache file when `cache_key` is given and holds this
/// session, else the Copilot API. The flag says the answer came from the
/// API and is worth caching.
fn fetch_log(
    client: &GithubClient,
    session_id: &str,
    cache_key: Option<&str>,
) -> Result<(SessionLog, bool), Error> {
    if let Some(log) = cache_key.and_then(logs::cached) {
        return Ok((log, false));
    }
    let log =
        client.copilot_agent_session_log(session_id, copilot_agent_sessions::MAX_LOG_BYTES)?;
    Ok((log, cache_key.is_some()))
}
