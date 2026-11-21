//! Everything the dashboard loads, and the watcher that keeps the monitored
//! tasks up to date. Every GitHub call blocks, so each one runs through
//! `cx.background_spawn`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, Context, SharedString, Task as GpuiTask};
use subspace_framework::client_github::Client as GithubClient;
use subspace_framework::client_github::copilot_agents::{Subject, Task, TaskDetail};
use subspace_framework::core::copilot_agent_tasks::{SubjectTaskRef, TaskStore};
use subspace_framework::core::http::Error;
use subspace_framework::core::keyed_request::Request;
use subspace_framework::core::task_state::{AlertKind, POLL_INTERVAL};
use subspace_framework::dashboard::{DashboardEvent, Toast, alerts, refresh};

use crate::model::Kind;
use crate::{CopilotAgents, watch};

/// What one list load returns: the task list, the saved references, which
/// are read from the same background thread, and the work login when the
/// load was asked to name it.
type ListResult = (
    Result<Vec<Task>, Error>,
    Vec<SubjectTaskRef>,
    Option<String>,
);

impl CopilotAgents {
    /// Loads the task list and the saved references, unless a load runs.
    pub(crate) fn load_all(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let (Some(client), store) = (self.client.clone(), self.store.clone()) else {
            self.error = Some("Subspace has no credential sources.".into());
            self.finish_load(cx);
            return;
        };
        self.loading = true;
        // The login does not change while the app runs, so one answer lasts.
        let name_login = self.login.is_none();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { list_tasks(&client, store, name_login) })
                .await;
            _ = this.update(cx, |this, cx| {
                this.apply_list(loaded, cx);
            });
        })
        .detach();
        self.rebuild(cx);
        cx.notify();
    }

    fn apply_list(&mut self, (tasks, saved, login): ListResult, cx: &mut Context<Self>) {
        self.loading = false;
        self.saved = saved;
        if login.is_some() {
            self.login = login;
        }
        match tasks {
            Ok(tasks) => {
                self.loaded_at = Some(Instant::now());
                self.error = None;
                self.tasks = tasks;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.finish_load(cx);
    }

    /// A load ended: rebuild, and seed the watcher's memory the first time so
    /// nothing that was already finished alerts.
    fn finish_load(&mut self, cx: &mut Context<Self>) {
        self.rebuild(cx);
        if self.previous.is_empty() {
            self.previous = watch::remembered(&self.all_rows);
        }
        cx.notify();
    }

    // ---- The open panel ------------------------------------------------------------

    /// Loads the sessions of the task the inspector or the opened tab shows.
    /// A local chat needs no request.
    pub(crate) fn load_detail(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(row) = self.row(key).filter(|row| row.kind == Kind::Cloud) else {
            self.detail_key = Some(key.to_owned());
            self.detail = None;
            self.detail_error = None;
            return;
        };
        let (task_id, subject) = (row.id.clone(), row.subject.clone());
        self.detail_key = Some(key.to_owned());
        self.detail = None;
        self.detail_error = None;
        let Some(client) = self.client.clone() else {
            self.detail_error = Some("Subspace has no credential sources.".into());
            return;
        };
        let request = self.detail_request.begin(key.to_owned());
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(
                    async move { client.get_copilot_agent_task_detail(&task_id, subject) },
                )
                .await;
            _ = this.update(cx, |this, cx| {
                this.apply_detail(request, loaded, cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// Takes a detail answer, unless a newer request or another row replaced
    /// what the panel wants.
    fn apply_detail(
        &mut self,
        request: Request<String>,
        loaded: Result<TaskDetail, Error>,
        cx: &mut Context<Self>,
    ) {
        if !self.detail_request.is_current(&request)
            || self.detail_key.as_ref() != Some(request.key())
        {
            return;
        }
        match loaded {
            Ok(detail) => {
                self.known
                    .insert(detail.task.id.clone(), detail.task.clone());
                crate::logs::load::prefetch_latest(
                    self.client.clone(),
                    request.key().clone(),
                    &detail,
                    cx,
                );
                self.detail = Some(detail);
                self.detail_error = None;
                self.rebuild(cx);
            }
            Err(error) => self.detail_error = Some(error.to_string()),
        }
        cx.notify();
    }

    // ---- The watcher ---------------------------------------------------------------

    /// What the next watcher round should do: the tasks to poll, and whether
    /// the whole list is due again.
    fn round(&self, cx: &gpui_kit::App) -> (Vec<String>, bool) {
        let interval = refresh::interval("github-copilot-agents", watch::LIST_REFRESH, cx);
        let due = self.loaded_at.is_none_or(|at| at.elapsed() >= interval)
            && watch::any_active(&self.all_rows);
        (watch::to_poll(&self.all_rows), due)
    }

    /// One watcher round: refresh the list when it is due, then ask GitHub
    /// about the monitored running tasks.
    fn poll(&mut self, cx: &mut Context<Self>) {
        if !refresh::enabled("github-copilot-agents", cx) {
            return;
        }
        let (ids, due) = self.round(cx);
        if due {
            self.load_all(cx);
        }
        self.reload_open_log(cx);
        let (Some(client), false) = (self.client.clone(), ids.is_empty()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let polled = cx
                .background_spawn(async move { poll_tasks(&client, &ids) })
                .await;
            _ = this.update(cx, |this, cx| this.apply_poll(polled, cx));
        })
        .detach();
    }

    /// Takes a poll answer: remembers each new state, alerts on the changes
    /// that earn one, and slows the next round after a failure.
    fn apply_poll(&mut self, polled: Vec<Result<Task, Error>>, cx: &mut Context<Self>) {
        let failed = polled.iter().any(Result::is_err);
        let tasks: Vec<Task> = polled.into_iter().flatten().collect();
        for task in &tasks {
            let alert = watch::alert(self.previous.get(&task.id), &task.state);
            self.previous.insert(task.id.clone(), task.state.clone());
            self.known.insert(task.id.clone(), task.clone());
            if let Some(kind) = alert {
                self.raise(task, kind, cx);
            }
        }
        if failed {
            self.backoff.failed();
        } else {
            self.backoff.succeeded();
        }
        self.rebuild(cx);
        cx.notify();
    }

    /// A monitored task changed: a toast with its link, a sound, and a
    /// notification while the window is in the background.
    fn raise(&self, task: &Task, kind: AlertKind, cx: &mut Context<Self>) {
        let text = watch::alert_text(&task.name, &task.repo, kind);
        cx.emit(DashboardEvent::Toast(toast(&text, task)));
        let settings = alerts::settings(cx);
        if settings.sound {
            alerts::play_sound(&settings.sound_name, cx);
        }
        if settings.notify && !self.window_active {
            alerts::notify("Copilot Agents", &text);
        }
    }
}

/// A toast that points at the task's pull request, or at its session page.
fn toast(text: &str, task: &Task) -> Toast {
    let link = task
        .pull_request
        .as_ref()
        .map(|pull| {
            (
                pull.url.clone(),
                SharedString::from("Pull request \u{2197}"),
            )
        })
        .or_else(|| {
            (!task.html_url.is_empty()).then(|| {
                (
                    task.html_url.clone(),
                    SharedString::from("Session \u{2197}"),
                )
            })
        });
    match link {
        Some((url, label)) => Toast::info(text.to_owned()).link(url, label),
        None => Toast::info(text.to_owned()),
    }
}

/// Records a monitor choice. A task the file already holds has every one of
/// its references updated; one it does not is recorded under its subject.
pub(crate) fn record_monitor(
    store: &TaskStore,
    row: &crate::model::AgentRow,
    monitor: bool,
) -> std::io::Result<()> {
    if store.set_monitor(&row.id, monitor)? {
        return Ok(());
    }
    let (subject, entry) = crate::filters::monitor_ref(row, monitor);
    store.record(&subject, entry)
}

/// The task list and the saved references, both blocking reads. With
/// `name_login`, a list that loaded also names the work login it was read
/// as: one more small request, which the navigator and the status bar show.
fn list_tasks(
    client: &GithubClient,
    store: Option<Arc<TaskStore>>,
    name_login: bool,
) -> ListResult {
    let tasks = client.list_my_copilot_agent_tasks().map(|list| list.tasks);
    let login = match name_login && tasks.is_ok() {
        true => client.work_login().ok(),
        false => None,
    };
    let saved = store.and_then(|store| store.all().ok()).unwrap_or_default();
    (tasks, saved, login)
}

/// One request per monitored task. The detail endpoint is the only one that
/// needs no repository path, so it serves as the cheap state read too.
fn poll_tasks(client: &GithubClient, ids: &[String]) -> Vec<Result<Task, Error>> {
    ids.iter()
        .map(|id| {
            client
                .get_copilot_agent_task_detail(id, None::<Subject>)
                .map(|detail| detail.task)
        })
        .collect()
}

/// The watcher loop. It keeps running while the dashboard lives, which is
/// for the whole session, so monitored tasks are followed while another
/// dashboard shows.
pub(crate) fn watcher(cx: &mut Context<CopilotAgents>) -> GpuiTask<()> {
    cx.spawn(async move |this, cx| {
        let mut last_poll = Instant::now();
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let result = this.update(cx, |this, cx| {
                let interval = refresh::interval("github-copilot-agents", POLL_INTERVAL, cx)
                    .max(this.backoff.interval());
                if refresh::enabled("github-copilot-agents", cx) && last_poll.elapsed() >= interval
                {
                    this.poll(cx);
                    last_poll = Instant::now();
                }
            });
            if result.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod refresh_tests;

#[cfg(test)]
mod detail_tests;

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{PullRequestArtifact, Task};
    use subspace_framework::client_github::model::PrState;

    use super::toast;

    fn task() -> Task {
        Task {
            id: "t1".into(),
            repo: "Owner/one".into(),
            name: "Fix".into(),
            state: "completed".into(),
            html_url: "https://github.com/Owner/one/agents/tasks/t1".into(),
            created_at: String::new(),
            updated_at: String::new(),
            pull_request: None,
            subject: None,
        }
    }

    #[test]
    fn a_toast_prefers_the_pull_request_then_the_session_page() {
        let plain = toast("Fix completed", &task());
        assert_eq!(plain.text, "Fix completed");
        assert_eq!(
            plain.link.as_ref().map(|link| link.label.to_string()),
            Some("Session \u{2197}".to_owned())
        );
        let mut with_pull = task();
        with_pull.pull_request = Some(PullRequestArtifact {
            number: 7,
            url: "https://github.com/Owner/one/pull/7".into(),
            state: PrState::Open,
        });
        let linked = toast("Fix completed", &with_pull);
        assert_eq!(
            linked.link.as_ref().map(|link| link.href.to_string()),
            Some("https://github.com/Owner/one/pull/7".to_owned())
        );
    }

    #[test]
    fn a_task_with_no_page_and_no_pull_request_gets_a_plain_toast() {
        let mut bare = task();
        bare.html_url = String::new();
        assert!(toast("Fix completed", &bare).link.is_none());
    }
}
