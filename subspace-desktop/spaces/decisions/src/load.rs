//! Reading Subspace API. Client calls block, so the Space runs them in the
//! background (`cx.background_spawn`) through a [`Reader`], which the tests
//! replace with fixture answers. Each read returns a [`Lease`] on the
//! account of the client that made it, and a list read carries a [`Stop`]
//! that a newer read sets.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use subspace_api_client::issue_writes::ChoiceSource;
use subspace_api_client::model::{DecisionDetail, DecisionListItem, Me, Project};
use subspace_api_client::{Client, DecisionQuery, Error};

use crate::words::People;

mod scope;

pub(crate) use scope::Continuation;

/// What a list read asks for: the decisions for this query, and first the
/// account, its projects and its people when `directory` is set. `stop`
/// ends the read before its next page once its answer is no longer wanted.
#[derive(Clone)]
pub(crate) struct Plan {
    pub(crate) query: DecisionQuery,
    pub(crate) probe: Option<crate::model::Choice>,
    pub(crate) directory: bool,
    pub(crate) stop: Stop,
    pub(crate) initiative: Option<String>,
    pub(crate) projects: Arc<[Project]>,
    pub(crate) next: Option<Arc<Continuation>>,
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
#[derive(Default)]
pub(crate) struct Found {
    pub(crate) items: Vec<DecisionListItem>,
    pub(crate) reveal: Option<crate::model::Choice>,
    pub(crate) next: Option<Arc<Continuation>>,
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
        let backend = subspace_api_client::backend();
        let mut read = |query: &DecisionQuery, page: &subspace_api_client::PageRequest| {
            client.decisions(query, page)
        };
        let found = scope::read(plan, projects, backend, &mut read).map(|mut found| {
            found.reveal = reveal(plan, &found, |wider| {
                scope::first(wider, projects, backend, &mut read)
                    .ok()
                    .flatten()
            });
            found
        });
        Answer { directory, found }
    })
}

/// Check for one match with the list filters relaxed but the same account,
/// initiative and search. A refusal or an absent match leaves the normal empty state.
pub(crate) fn reveal(
    plan: &Plan,
    found: &Found,
    match_looking_wider: impl FnOnce(&Plan) -> Option<DecisionListItem>,
) -> Option<crate::model::Choice> {
    let choice = plan.probe.as_ref()?;
    if !found.items.is_empty() || found.next.is_some() || plan.stop.stopped() {
        return None;
    }
    let mut wider = plan.clone();
    wider.query = DecisionQuery::default();
    wider.query.q.clone_from(&plan.query.q);
    let item = match_looking_wider(&wider)?;
    let reveal = choice.revealing(&item);
    (reveal != *choice).then_some(reveal)
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
    let projects = directory_projects(client.all_projects(), projects)?;
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

/// Project permissions are independent of permission to read decisions.
fn directory_projects(
    result: Result<Vec<Project>, Error>,
    held: &[Project],
) -> Result<Vec<Project>, Error> {
    match result {
        Ok(projects) => Ok(projects),
        Err(Error::Forbidden) => Ok(Vec::new()),
        Err(error @ (Error::SignInNeeded | Error::OrganisationNeeded)) => Err(error),
        Err(_) => Ok(held.to_vec()),
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use subspace_api_client::Error;

    use super::{Failure, Leased, directory_projects, failure, leased};
    use crate::fixtures;

    #[test]
    fn a_project_directory_failure_is_independent_of_the_decisions_session() {
        let held = fixtures::projects();
        for error in [Error::Network, Error::UnexpectedStatus(502)] {
            let retained = directory_projects(Err(error), &held).unwrap();
            assert_eq!(retained.len(), held.len());
        }
        assert!(
            directory_projects(Err(Error::Forbidden), &held)
                .unwrap()
                .is_empty()
        );
        for error in [Error::SignInNeeded, Error::OrganisationNeeded] {
            assert_eq!(directory_projects(Err(error), &held).err(), Some(error));
        }
    }

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
