//! Reading Subspace API. Client calls block, so the Space runs them in the
//! background (`cx.background_spawn`) through a [`Reader`], which the tests
//! replace with fixture answers. Each read returns a [`Lease`] on the
//! account of the client that made it, and a list read carries a [`Stop`]
//! that a newer read sets.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use subspace_api_client::issue_writes::ChoiceSource;
use subspace_api_client::model::{DecisionDetail, DecisionListItem, Me, Page, Project};
use subspace_api_client::{Client, DecisionQuery, Error, MAX_DECISION_PAGE_SIZE, PageRequest};

use crate::words::People;

/// The list reads at most this many pages, a thousand decisions; the status
/// says when there are more.
const MAX_PAGES: usize = 10;

/// What a list read asks for: the decisions for this query, and first the
/// account, its projects and its people when `directory` is set. `stop`
/// ends the read before its next page once its answer is no longer wanted.
#[derive(Clone)]
pub(crate) struct Plan {
    pub(crate) query: DecisionQuery,
    pub(crate) directory: bool,
    pub(crate) stop: Stop,
    pub(crate) initiative: Option<String>,
    pub(crate) projects: Arc<[Project]>,
}

/// Set when a newer read, or another account, supersedes a list read: the
/// read stops before its next page, and its answer is dropped.
#[derive(Clone, Default)]
pub(crate) struct Stop(Arc<AtomicBool>);

impl Stop {
    pub(crate) fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(crate) fn stopped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The account and organisation a read was made for. Once either changes,
/// what the read found belongs to no one on screen.
#[derive(Clone)]
pub(crate) struct Lease(Arc<dyn Fn() -> bool + Send + Sync>);

impl Lease {
    /// A lease that ends once `moved` answers true.
    pub(crate) fn new(moved: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(moved))
    }

    /// Whether the signed-in account or organisation changed since the
    /// lease was taken, including a sign-out.
    pub(crate) fn moved(&self) -> bool {
        (self.0)()
    }
}

/// The signed-in account, the projects it can see and its people's names.
pub(crate) struct Directory {
    pub(crate) me: Me,
    pub(crate) projects: Vec<Project>,
    pub(crate) people: People,
    pub(crate) human_makers: Vec<(String, String)>,
}

/// The decisions a list read found, newest decided first, and whether more
/// were left unread.
pub(crate) struct Found {
    pub(crate) items: Vec<DecisionListItem>,
    pub(crate) more: bool,
}

/// One list read's answer.
pub(crate) struct Answer {
    pub(crate) directory: Option<Result<Directory, Error>>,
    pub(crate) found: Result<Found, Error>,
}

/// What one read returned. A read through a client comes with a lease on
/// that client's own account, so whatever it found is watched for the
/// account it was read for. A read that could make no client read nothing
/// for any account: it has no lease, and only its refusal.
pub(crate) enum Leased<T> {
    Read(Lease, T),
    Unread(Error),
}

impl<T> Leased<T> {
    /// The read's lease, if it had a client, and its value, or for a read
    /// without a client the value `refused` makes of its error.
    pub(crate) fn split(self, refused: impl FnOnce(Error) -> T) -> (Option<Lease>, T) {
        match self {
            Leased::Read(lease, value) => (Some(lease), value),
            Leased::Unread(error) => (None, refused(error)),
        }
    }
}

pub(crate) type ListRead = Arc<dyn Fn(&Plan) -> Leased<Answer> + Send + Sync>;
pub(crate) type DetailRead =
    Arc<dyn Fn(&str, Option<&Lease>) -> Leased<Result<DecisionDetail, Error>> + Send + Sync>;

/// The Space's two reads, a list and one decision, each returning a lease
/// on the account of the client that made it.
#[derive(Clone)]
pub(crate) struct Reader {
    pub(crate) list: ListRead,
    pub(crate) detail: DetailRead,
}

impl Reader {
    /// The reads of the signed-in account's Subspace API.
    pub(crate) fn service() -> Self {
        Self {
            list: Arc::new(read_list),
            detail: Arc::new(read_detail),
        }
    }
}

/// Reads through `client` under a lease on that same client's account,
/// which `changed` says has since changed. The lease is as old as the
/// client, so an account change at any point after the client was made,
/// during the read included, ends it. Without a client nothing is read.
pub(crate) fn leased<C, T>(
    client: Result<C, Error>,
    changed: fn(&C) -> bool,
    read: impl FnOnce(&C) -> T,
) -> Leased<T>
where
    C: Send + Sync + 'static,
{
    match client {
        Ok(client) => {
            let value = read(&client);
            Leased::Read(Lease::new(move || changed(&client)), value)
        }
        Err(error) => Leased::Unread(error),
    }
}

/// Why a read failed, as the Space words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    SignIn,
    Forbidden,
    Organisation,
    Network,
    Unexpected,
}

impl Failure {
    /// A failure that says nothing read may stay on screen.
    pub(crate) fn account_level(self) -> bool {
        matches!(
            self,
            Failure::SignIn | Failure::Forbidden | Failure::Organisation
        )
    }

    /// Each failure's title, in the order the variants are declared, so a
    /// failure's discriminant is its entry.
    const TITLES: [&'static str; 5] = [
        "Sign-in required",
        "Access denied",
        "Choose an organisation",
        "Offline",
        "Couldn't load decisions",
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Failure::SignIn => subspace_api_client::sign_in_state().0,
            _ => Self::TITLES[self as usize],
        }
    }

    pub(crate) fn message(self) -> String {
        let service = subspace_api_client::service_name();
        match self {
            Failure::SignIn => subspace_api_client::sign_in_state().1.to_owned(),
            Failure::Forbidden => {
                format!("{service} refused access. Try again after access is restored.")
            }
            Failure::Organisation => Error::OrganisationNeeded.to_string(),
            Failure::Network => subspace_api_client::unreachable_message(),
            Failure::Unexpected => format!("{service} couldn't be read. Try again."),
        }
    }
}

pub(crate) fn failure(error: Error) -> Failure {
    match error {
        Error::SignInNeeded => Failure::SignIn,
        Error::Forbidden => Failure::Forbidden,
        Error::OrganisationNeeded => Failure::Organisation,
        Error::Network => Failure::Network,
        Error::NotFound
        | Error::UnexpectedStatus(_)
        | Error::InvalidResponse
        | Error::InvalidRequest => Failure::Unexpected,
    }
}

fn read_list(plan: &Plan) -> Leased<Answer> {
    leased(Client::new(), Client::account_changed, |client| {
        let directory = plan
            .directory
            .then(|| read_directory(client, &plan.projects));
        if let Some(Err(error)) = &directory {
            return refused(plan, *error);
        }
        let projects = directory
            .as_ref()
            .and_then(|directory| directory.as_ref().ok())
            .map_or(plan.projects.as_ref(), |directory| &directory.projects);
        let found = read_decisions(client, plan, projects);
        Answer { directory, found }
    })
}

fn read_decisions(client: &Client, plan: &Plan, projects: &[Project]) -> Result<Found, Error> {
    let Some(initiative) = plan.initiative.as_deref() else {
        return all_pages(&plan.stop, |page| client.decisions(&plan.query, page));
    };
    let keys: HashSet<_> = projects
        .iter()
        .filter(|project| crate::initiative::includes(project, Some(initiative)))
        .map(|project| project.key.as_str())
        .collect();
    if keys.is_empty() {
        return Ok(Found {
            items: Vec::new(),
            more: false,
        });
    }
    let mut query = plan.query.clone();
    if query
        .project
        .as_deref()
        .is_some_and(|key| !keys.contains(key))
    {
        query.project = None;
    }
    if query.project.is_none() && keys.len() == 1 {
        query.project = keys.iter().next().map(|key| (*key).to_owned());
    }
    all_pages(&plan.stop, |page| {
        let mut reply = client.decisions(&query, page)?;
        reply
            .items
            .retain(|item| keys.contains(item.project_key.as_str()));
        Ok(reply)
    })
}

/// Every part of the answer failed with `error`.
fn refused(plan: &Plan, error: Error) -> Answer {
    Answer {
        directory: plan.directory.then_some(Err(error)),
        found: Err(error),
    }
}

/// The account first: without it nothing else is read. A temporary project
/// read failure leaves the directory already read in this account available.
fn read_directory(client: &Client, projects: &[Project]) -> Result<Directory, Error> {
    let me = client.me()?;
    let choices = client
        .issue_choices(&ChoiceSource::Assignees)
        .unwrap_or_default();
    let human_makers = choices
        .iter()
        .filter_map(|choice| Some((choice.human_id()?.to_owned(), choice.name().to_owned())))
        .collect();
    let projects = match client.all_projects() {
        Ok(projects) => projects,
        Err(Error::Network) => projects.to_vec(),
        Err(error) => return Err(error),
    };
    Ok(Directory {
        me,
        projects,
        people: choices
            .into_iter()
            .map(|choice| (choice.id().to_owned(), choice.name().to_owned()))
            .collect(),
        human_makers,
    })
}

pub(crate) fn bound_client(lease: Option<&Lease>) -> Result<Client, Error> {
    let moved = || lease.is_some_and(Lease::moved);
    if moved() {
        return Err(Error::SignInNeeded);
    }
    let client = Client::new()?;
    if moved() {
        return Err(Error::SignInNeeded);
    }
    Ok(client)
}

fn read_detail(id: &str, lease: Option<&Lease>) -> Leased<Result<DecisionDetail, Error>> {
    leased(bound_client(lease), Client::account_changed, |client| {
        client.decision(id)
    })
}

/// Reads pages with `read` until the last, or until [`MAX_PAGES`], and
/// refuses a cursor that repeats. A read that `stop` ends asks for no
/// further page; its answer is dropped, so what it returns does not matter.
pub(crate) fn all_pages(
    stop: &Stop,
    mut read: impl FnMut(&PageRequest) -> Result<Page<DecisionListItem>, Error>,
) -> Result<Found, Error> {
    let mut page = PageRequest::default();
    page.limit = MAX_DECISION_PAGE_SIZE;
    let mut items = Vec::new();
    let mut cursors = HashSet::new();
    for _ in 0..MAX_PAGES {
        if stop.stopped() {
            return Ok(Found { items, more: true });
        }
        let found = read(&page)?;
        items.extend(found.items);
        let Some(cursor) = found.next_cursor else {
            return Ok(Found { items, more: false });
        };
        if !cursors.insert(cursor.clone()) {
            return Err(Error::InvalidResponse);
        }
        page.cursor = Some(cursor);
    }
    Ok(Found { items, more: true })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use serde_json::json;
    use subspace_api_client::Error;
    use subspace_api_client::model::{DecisionListItem, Page};

    use super::{Failure, Leased, Stop, all_pages, failure, leased};
    use crate::fixtures;

    /// A client whose account the test changes, as a sign-out or a switch
    /// of account or organisation does.
    struct Watched(Arc<AtomicBool>);

    fn changed(client: &Watched) -> bool {
        client.0.load(Ordering::SeqCst)
    }

    #[test]
    fn a_read_is_leased_on_the_account_of_its_own_client() {
        let switched = Arc::new(AtomicBool::new(false));
        let during = switched.clone();
        let read = leased(Ok(Watched(switched.clone())), changed, |_| {
            // The account changes while the read runs, after its client
            // was made: the answer it returns belongs to no one on screen.
            during.store(true, Ordering::SeqCst);
            "found"
        });
        let Leased::Read(lease, found) = read else {
            panic!("a read through a client comes with that client's lease");
        };
        assert_eq!(found, "found");
        assert!(lease.moved(), "the change after the client ends its lease");
        switched.store(false, Ordering::SeqCst);
        assert!(!lease.moved(), "the lease watches that same client");
    }

    #[test]
    fn a_read_that_can_make_no_client_reads_nothing_and_has_no_lease() {
        let mut asked = false;
        let read = leased(Err::<Watched, _>(Error::InvalidRequest), changed, |_| {
            asked = true;
            "found"
        });
        assert!(!asked, "nothing is read without a client");
        let (lease, found) = read.split(|_| "refused");
        assert!(lease.is_none());
        assert_eq!(found, "refused");
    }

    fn page(ids: &[&str], next: Option<&str>) -> Page<DecisionListItem> {
        let item = |id: &str| fixtures::item_value(id, "NAV", "Fixture", "accepted", "2026-11-19");
        let items: Vec<_> = ids.iter().map(|id| item(id)).collect();
        serde_json::from_value(json!({"items": items, "nextCursor": next})).unwrap()
    }

    #[test]
    fn every_page_is_read_until_the_last() {
        let mut asked = Vec::new();
        let found = all_pages(&Stop::default(), |request| {
            asked.push((request.limit, request.cursor.clone()));
            Ok(match request.cursor.as_deref() {
                None => page(&["a", "b"], Some("next")),
                _ => page(&["c"], None),
            })
        })
        .unwrap();
        let ids: Vec<_> = found.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert!(!found.more);
        assert_eq!(asked, [(100, None), (100, Some("next".to_owned()))]);
    }

    #[test]
    fn a_repeated_cursor_is_refused_and_a_long_list_stops_with_more() {
        let go = Stop::default();
        let repeated = all_pages(&go, |_| Ok(page(&["a"], Some("same"))));
        assert!(matches!(repeated, Err(Error::InvalidResponse)));
        let mut cursor = 0;
        let long = all_pages(&go, |_| {
            cursor += 1;
            Ok(page(&["a"], Some(cursor.to_string().as_str())))
        })
        .unwrap();
        assert!(long.more);
        assert_eq!(long.items.len(), 10);
        assert!(all_pages(&go, |_| Err(Error::Network)).is_err());
    }

    #[test]
    fn a_stopped_read_asks_for_no_further_page() {
        let stop = Stop::default();
        let mut asked = 0;
        let found = all_pages(&stop, |_| {
            asked += 1;
            // A newer read supersedes this one while its first page loads.
            stop.stop();
            Ok(page(&["a"], Some(asked.to_string().as_str())))
        })
        .unwrap();
        assert_eq!(asked, 1, "no page after the stop");
        assert_eq!(found.items.len(), 1);
    }

    #[test]
    fn a_failure_is_worded_and_account_failures_clear_the_list() {
        assert_eq!(failure(Error::SignInNeeded), Failure::SignIn);
        assert_eq!(failure(Error::UnexpectedStatus(503)), Failure::Unexpected);
        assert_eq!(failure(Error::Network), Failure::Network);
        assert!(Failure::SignIn.account_level() && Failure::Organisation.account_level());
        assert!(!Failure::Network.account_level() && !Failure::Unexpected.account_level());
        assert_eq!(Failure::Network.title(), "Offline");
        assert_eq!(
            Failure::Network.message(),
            "Couldn't reach Subspace API. Try again."
        );
    }
}
