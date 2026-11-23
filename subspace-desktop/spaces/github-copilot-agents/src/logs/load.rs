//! Reading one session log: what the Show log action starts, the cache file
//! it prefers, and the refresh the watcher's round asks for. Every read
//! blocks, so each one runs through `cx.background_spawn`.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::Client as GithubClient;
use subspace_framework::client_github::PinnedWork;
use subspace_framework::client_github::copilot_agent_sessions::{self, SessionLog};
use subspace_framework::core::http::{Error, status};
use subspace_framework::core::task_state::is_active;
use subspace_framework::dashboard::{full_text, full_text_service, prefs};

use crate::CopilotAgents;
use crate::actions::say;
use crate::load::{ACCOUNT_CHANGED, WorkIdentity};
use crate::logs::{self, LogState, OpenLog};

/// The search board this Space's session logs are indexed in.
const BOARD: &str = "github-copilot-agents";

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
    /// reads apart. The read is given the account the Space shows, and its
    /// search activation ticket is taken before anything else, so a read
    /// overtaken by another can neither index nor activate a search scope.
    fn read_log(&mut self, cache: bool, cx: &mut Context<Self>) {
        let (Some(open), Some(client)) = (self.log.clone(), self.client.clone()) else {
            return;
        };
        self.log_request = self.log_request.wrapping_add(1);
        let request = self.log_request;
        let key = cache.then(|| open.cache_key.clone());
        let expected = self.identity.clone();
        let search = full_text_service(cx);
        let links = self
            .detail
            .as_ref()
            .map(logs::TaskLinks::from_detail)
            .unwrap_or_else(|| logs::TaskLinks::from_task_key(&open.task_key));
        cx.spawn(async move |this, cx| {
            let (loaded, indexed) = cx
                .background_spawn(async move {
                    let ticket = search
                        .as_ref()
                        .and_then(|service| Some((service, begin_index(service).ok()??)));
                    let loaded =
                        fetch_bound(&client, expected.as_ref(), &open.session_id, key.as_deref());
                    let indexed = loaded.as_ref().ok().zip(ticket).and_then(
                        |((log, _, identity), (service, ticket))| {
                            index_log(
                                service,
                                &ticket,
                                identity,
                                LogIndex {
                                    task_key: &open.task_key,
                                    session_id: &open.session_id,
                                    links: &links,
                                    log,
                                },
                            )
                            .err()
                        },
                    );
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
    /// A log read as another account than the one the Space shows is not
    /// shown or cached either, and the list is read again so the Space
    /// follows the account the credential now holds. An API answer is
    /// cached under the account it was read as.
    pub(crate) fn apply_log(
        &mut self,
        request: u64,
        loaded: Result<BoundLog, Error>,
        cx: &mut Context<Self>,
    ) {
        if request != self.log_request {
            return;
        }
        let shown = self.identity.clone();
        let Some(open) = self.log.as_mut() else {
            return;
        };
        let mut reload = false;
        match loaded {
            Ok((log, fresh, identity)) if shows(shown.as_ref(), &identity) => {
                let key = logs::account_key(&identity.account_id, &open.cache_key);
                let text = fresh.then(|| logs::encode(&key, &log)).flatten();
                open.state = LogState::Ready(log);
                if let Some(text) = text {
                    prefs::write_cache(logs::CACHE_FILE, text, cx);
                }
            }
            Ok(_) => {
                open.state = LogState::Unavailable;
                reload = true;
            }
            Err(error) => {
                open.state = LogState::Unavailable;
                reload = error.to_string() == ACCOUNT_CHANGED;
            }
        }
        if reload {
            self.load_all(cx);
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

/// Whether a log read as `read` may show in a Space that shows `shown`:
/// only when it is that same account, or the Space names none yet.
fn shows(shown: Option<&WorkIdentity>, read: &WorkIdentity) -> bool {
    shown.is_none_or(|shown| shown.account_id == read.account_id)
}

/// The search activation ticket a log read takes before it pins its work
/// credential, when the search index takes this Space's logs. A later
/// read's ticket cancels it, so an overtaken read can activate no scope.
fn begin_index(
    service: &full_text::Service,
) -> Result<Option<full_text::ActivationTicket>, String> {
    if !indexes_logs(service)? {
        return Ok(None);
    }
    service.begin_activation(BOARD).map(Some)
}

/// Indexes a log under the search scope of `identity`, the work account it
/// was read as (see [`fetch_bound`]), never under whichever account holds
/// the credential once the read is over. `ticket` was taken when the read
/// began: when another read began since, this one was overtaken, so it
/// activates no scope and indexes nothing, which is not a failure; when
/// another read activated its scope since this lease, the write is refused.
fn index_log(
    service: &full_text::Service,
    ticket: &full_text::ActivationTicket,
    identity: &WorkIdentity,
    request: LogIndex<'_>,
) -> Result<(), String> {
    let scope =
        logs::scope(service.app_id(), &identity.account_id).ok_or("Missing GitHub identity")?;
    let Ok(activation) = service.prepare_activation_ticket(ticket, &scope) else {
        return Ok(());
    };
    if service.activate_prepared(&activation).is_err() {
        return Ok(());
    }
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
    Ok(service.policy(BOARD)?.enabled)
}

/// Reads a loaded task's last session log for the search index, without
/// showing it. Nothing is read unless session logs are on and the search
/// index takes this Space's logs, so selecting a task reads no log the
/// index would not keep. Like a shown read, it is given the account the
/// Space shows and takes its activation ticket first.
pub(crate) fn prefetch_latest(
    client: Option<Arc<GithubClient>>,
    expected: Option<WorkIdentity>,
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
                    || begin_index(&service),
                    || {
                        fetch_bound(&client, expected.as_ref(), &session.id, Some(&key))
                            .map(|(log, _, identity)| (log, identity))
                            .map_err(|error| error.to_string())
                    },
                    |ticket, (log, identity)| {
                        index_log(
                            &service,
                            ticket,
                            identity,
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

/// The prefetch's order: the index policy and the activation ticket first,
/// and the log only when the index will keep it.
fn prefetch<K, T>(
    begin: impl FnOnce() -> Result<Option<K>, String>,
    fetch: impl FnOnce() -> Result<T, String>,
    index: impl FnOnce(&K, &T) -> Result<(), String>,
) -> Result<(), String> {
    let Some(ticket) = begin()? else {
        return Ok(());
    };
    index(&ticket, &fetch()?)
}

/// A log, whether it came from the API and is worth caching, and the work
/// account it was read as.
pub(crate) type BoundLog = (SessionLog, bool, WorkIdentity);

/// What a log read needs of the work credential it is pinned to.
trait LogSource {
    /// The account GitHub verified the credential belongs to.
    fn identity(&self) -> WorkIdentity;
    /// The session's log, read with that credential.
    fn session_log(&self, session_id: &str) -> Result<SessionLog, Error>;
}

impl LogSource for PinnedWork<'_> {
    fn identity(&self) -> WorkIdentity {
        WorkIdentity::of(self)
    }

    fn session_log(&self, session_id: &str) -> Result<SessionLog, Error> {
        self.copilot_agent_session_log(session_id, copilot_agent_sessions::MAX_LOG_BYTES)
    }
}

/// One log read pinned to one work credential ([`GithubClient::pin_work`]):
/// the account is looked up once, or not at all when this credential was
/// verified earlier this run, and the cache lookup, the API read and the
/// account the answer is cached and indexed under are all that
/// credential's.
fn fetch_bound(
    client: &GithubClient,
    expected: Option<&WorkIdentity>,
    session_id: &str,
    cache_key: Option<&str>,
) -> Result<BoundLog, Error> {
    let work = client.pin_work()?;
    read_pinned(&work, expected, session_id, cache_key, logs::cached)
}

/// One log read as `work`'s account: refused when that is not the account
/// the Space shows (`expected`), else the cache file when `cache_key` is
/// given and holds this session as read by that account, with no request,
/// else the Copilot API. The flag says the answer came from the API and is
/// worth caching.
fn read_pinned(
    work: &impl LogSource,
    expected: Option<&WorkIdentity>,
    session_id: &str,
    cache_key: Option<&str>,
    cached: impl FnOnce(&str) -> Option<SessionLog>,
) -> Result<BoundLog, Error> {
    let identity = work.identity();
    if !shows(expected, &identity) {
        return Err(Error::Status {
            status: status::BAD_GATEWAY,
            message: ACCOUNT_CHANGED.into(),
        });
    }
    let hit = cache_key.and_then(|key| cached(&logs::account_key(&identity.account_id, key)));
    if let Some(log) = hit {
        return Ok((log, false, identity));
    }
    let log = work.session_log(session_id)?;
    Ok((log, cache_key.is_some(), identity))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use subspace_framework::client_github::copilot_agent_sessions::SessionLog;
    use subspace_framework::core::http::Error;
    use subspace_framework::dashboard::full_text::{Policy, Service};

    use super::{BOARD, LogIndex, LogSource, begin_index, index_log, prefetch, read_pinned};
    use crate::load::{ACCOUNT_CHANGED, WorkIdentity};
    use crate::logs;

    fn identity(login: &str, account_id: &str) -> WorkIdentity {
        WorkIdentity {
            login: login.into(),
            account_id: account_id.into(),
        }
    }

    fn empty_log() -> SessionLog {
        SessionLog {
            entries: Vec::new(),
            truncated: false,
        }
    }

    /// A pinned work credential of one account that counts its API reads.
    struct Pinned {
        identity: WorkIdentity,
        reads: Cell<usize>,
    }

    impl Pinned {
        fn new(login: &str, account_id: &str) -> Self {
            Self {
                identity: identity(login, account_id),
                reads: Cell::new(0),
            }
        }
    }

    impl LogSource for Pinned {
        fn identity(&self) -> WorkIdentity {
            self.identity.clone()
        }

        fn session_log(&self, _: &str) -> Result<SessionLog, Error> {
            self.reads.set(self.reads.get() + 1);
            Ok(empty_log())
        }
    }

    const KEY: &str = "s1@2026-10-01T09:00:00Z";

    /// A cache file that holds this session as read by `account_id` only.
    fn cache_of(account_id: &str) -> impl FnOnce(&str) -> Option<SessionLog> + '_ {
        move |key| (key == logs::account_key(account_id, KEY)).then(empty_log)
    }

    /// The pin is the read's only lookup, and a cached log then needs no
    /// request at all, so it shows with GitHub unreachable once the
    /// credential was verified this run. Before, a cache hit needed two
    /// lookups of the work login, and offline it read as unavailable.
    #[test]
    fn a_cached_log_is_shown_without_a_request() {
        let work = Pinned::new("octo-dev", "U_dev");
        let (_, fresh, read_as) =
            read_pinned(&work, None, "s1", Some(KEY), cache_of("U_dev")).unwrap();
        assert!(!fresh, "a cached log is not cached again");
        assert_eq!(read_as, identity("octo-dev", "U_dev"));
        assert_eq!(work.reads.get(), 0, "no API read");
    }

    #[test]
    fn a_log_another_account_cached_is_read_again() {
        let work = Pinned::new("octo-dev", "U_dev");
        let (_, fresh, read_as) =
            read_pinned(&work, None, "s1", Some(KEY), cache_of("U_other")).unwrap();
        assert!(fresh, "an API answer is worth caching");
        assert_eq!(read_as.account_id, "U_dev");
        assert_eq!(work.reads.get(), 1);

        let (_, fresh, _) = read_pinned(&work, None, "s1", None, cache_of("U_dev")).unwrap();
        assert!(!fresh, "a refresh neither reads nor writes the cache");
        assert_eq!(work.reads.get(), 2);
    }

    /// A read whose pinned credential is another account's than the one
    /// the Space shows reads nothing, from the cache or the API.
    #[test]
    fn a_read_as_another_account_than_the_space_shows_is_refused() {
        let work = Pinned::new("octo-other", "U_other");
        let shown = identity("octo-dev", "U_dev");
        let asked = Cell::new(false);
        let error = read_pinned(&work, Some(&shown), "s1", Some(KEY), |_| {
            asked.set(true);
            None
        })
        .unwrap_err();
        assert_eq!(error.to_string(), ACCOUNT_CHANGED);
        assert!(!asked.get(), "no cache lookup");
        assert_eq!(work.reads.get(), 0, "no API read");

        let same = identity("Octo-Other", "U_other");
        assert!(read_pinned(&work, Some(&same), "s1", None, |_| None).is_ok());
    }

    /// A prefetch indexes its log under the account it was read as.
    #[test]
    fn a_prefetched_log_is_indexed_under_the_account_it_was_read_as() {
        let work = Pinned::new("octo-dev", "U_dev");
        let indexed_as = RefCell::new(None);
        let result = prefetch(
            || Ok(Some(())),
            || {
                read_pinned(&work, None, "s1", Some(KEY), |_| None)
                    .map(|(log, _, read_as)| (log, read_as))
                    .map_err(|error| error.to_string())
            },
            |_, (_, read_as): &(SessionLog, WorkIdentity)| {
                *indexed_as.borrow_mut() = Some(read_as.account_id.clone());
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(indexed_as.into_inner().as_deref(), Some("U_dev"));
    }

    #[test]
    fn a_prefetch_reads_no_log_the_search_index_would_not_keep() {
        let fetched = Cell::new(false);
        let indexed = Cell::new(false);
        let skipped = prefetch(
            || Ok(None::<()>),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_, _| {
                indexed.set(true);
                Ok(())
            },
        );
        assert_eq!(skipped, Ok(()));
        assert!(!fetched.get(), "the index is off, so no log is read");
        assert!(!indexed.get());

        let failed = prefetch(
            || Err::<Option<()>, _>("no index".into()),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_, _| Ok(()),
        );
        assert_eq!(failed, Err("no index".to_owned()));
        assert!(!fetched.get(), "an unreadable policy reads no log either");

        let kept = prefetch(
            || Ok(Some(())),
            || {
                fetched.set(true);
                Ok(empty_log())
            },
            |_, _| {
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

    fn indexing(root: &std::path::Path) -> Service {
        let _ = std::fs::remove_dir_all(root);
        let service = Service::open(root.into(), "app".into(), vec![BOARD.into()]).unwrap();
        let defaults = Policy::default();
        service
            .set_policy(
                BOARD,
                Policy {
                    enabled: true,
                    retention_days: defaults.retention_days,
                    max_bytes: defaults.max_bytes,
                },
            )
            .unwrap();
        service
    }

    /// An A read takes its activation ticket when it begins and pauses
    /// before indexing; the account changes and a B read begins and indexes
    /// meanwhile. When A resumes it neither indexes its log nor activates
    /// A's search scope during B's session. Before, a read took its ticket
    /// after fetching, so the late A read got a newer ticket than B's and
    /// reactivated A's scope.
    #[test]
    fn an_overtaken_read_neither_indexes_nor_activates_its_account_scope() {
        let root =
            std::env::temp_dir().join(format!("subspace-copilot-log-scope-{}", std::process::id()));
        let service = indexing(&root);
        let (dev, other) = (
            identity("octo-dev", "U_dev"),
            identity("octo-other", "U_other"),
        );
        let links = logs::TaskLinks::from_task_key("cloud:t1");
        let log = empty_log();
        let request = || LogIndex {
            task_key: "cloud:t1",
            session_id: "s1",
            links: &links,
            log: &log,
        };

        let first = begin_index(&service)
            .unwrap()
            .expect("the index takes logs");
        let second = begin_index(&service)
            .unwrap()
            .expect("the index takes logs");
        index_log(&service, &second, &other, request()).unwrap();
        assert_eq!(
            index_log(&service, &first, &dev, request()),
            Ok(()),
            "an overtaken read is not a failure"
        );

        let scope = |account: &str| logs::scope("app", account).unwrap();
        assert!(
            service.lease(&scope("U_other")).is_ok(),
            "B's scope stays active"
        );
        assert!(
            service.lease(&scope("U_dev")).is_err(),
            "A's scope was never activated"
        );
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_ticket_is_taken_while_the_index_takes_no_logs() {
        let root =
            std::env::temp_dir().join(format!("subspace-copilot-log-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let service = Service::open(root.clone(), "app".into(), vec![BOARD.into()]).unwrap();
        assert!(begin_index(&service).unwrap().is_none());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
}
