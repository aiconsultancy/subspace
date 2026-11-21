//! Reading Subspace API. Client calls block, so the Space runs them in the
//! background (`cx.background_spawn`) through a [`Reader`], which the tests
//! replace with fixture answers.

use std::collections::HashSet;
use std::sync::Arc;

use subspace_api_client::issue_writes::ChoiceSource;
use subspace_api_client::model::{DecisionDetail, DecisionListItem, Me, Page, Project};
use subspace_api_client::{Client, DecisionQuery, Error, MAX_DECISION_PAGE_SIZE, PageRequest};

use crate::words::People;

/// The list reads at most this many pages, a thousand decisions; the status
/// says when there are more.
const MAX_PAGES: usize = 10;

/// What a list read asks for: the decisions for this query, and first the
/// account, its projects and its people when `directory` is set.
#[derive(Clone)]
pub(crate) struct Plan {
    pub(crate) query: DecisionQuery,
    pub(crate) directory: bool,
}

/// The signed-in account, the projects it can see and its people's names.
pub(crate) struct Directory {
    pub(crate) me: Me,
    pub(crate) projects: Vec<Project>,
    pub(crate) people: People,
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

pub(crate) type ListRead = Arc<dyn Fn(&Plan) -> Answer + Send + Sync>;
pub(crate) type DetailRead = Arc<dyn Fn(&str) -> Result<DecisionDetail, Error> + Send + Sync>;

/// The Space's two reads: a list and one decision.
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
        Self::TITLES[self as usize]
    }

    pub(crate) fn message(self) -> String {
        let service = subspace_api_client::service_name();
        match self {
            Failure::SignIn => "Sign in under Settings › Account, then try again.".to_owned(),
            Failure::Forbidden => {
                format!("{service} refused access. Try again after access is restored.")
            }
            Failure::Organisation => Error::OrganisationNeeded.to_string(),
            Failure::Network => format!("Couldn't reach {service}. Try again."),
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

fn read_list(plan: &Plan) -> Answer {
    let client = match Client::new() {
        Ok(client) => client,
        Err(error) => return refused(plan, error),
    };
    let directory = plan.directory.then(|| read_directory(&client));
    if let Some(Err(error)) = &directory {
        return refused(plan, *error);
    }
    Answer {
        directory,
        found: all_pages(|page| client.decisions(&plan.query, page)),
    }
}

/// Every part of the answer failed with `error`.
fn refused(plan: &Plan, error: Error) -> Answer {
    Answer {
        directory: plan.directory.then_some(Err(error)),
        found: Err(error),
    }
}

/// The account first: without it nothing else is read. A project or people
/// read that fails leaves those parts empty.
fn read_directory(client: &Client) -> Result<Directory, Error> {
    let me = client.me()?;
    Ok(Directory {
        me,
        projects: client.all_projects().unwrap_or_default(),
        people: client
            .issue_choices(&ChoiceSource::Assignees)
            .map(|choices| {
                choices
                    .into_iter()
                    .map(|choice| (choice.id().to_owned(), choice.name().to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn read_detail(id: &str) -> Result<DecisionDetail, Error> {
    Client::new()?.decision(id)
}

/// Reads pages with `read` until the last, or until [`MAX_PAGES`], and
/// refuses a cursor that repeats.
pub(crate) fn all_pages(
    mut read: impl FnMut(&PageRequest) -> Result<Page<DecisionListItem>, Error>,
) -> Result<Found, Error> {
    let mut page = PageRequest::default();
    page.limit = MAX_DECISION_PAGE_SIZE;
    let mut items = Vec::new();
    let mut cursors = HashSet::new();
    for _ in 0..MAX_PAGES {
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
    use serde_json::json;
    use subspace_api_client::Error;
    use subspace_api_client::model::{DecisionListItem, Page};

    use super::{Failure, all_pages, failure};
    use crate::fixtures;

    fn page(ids: &[&str], next: Option<&str>) -> Page<DecisionListItem> {
        let item = |id: &str| fixtures::item_value(id, "NAV", "Fixture", "accepted", "2026-11-19");
        let items: Vec<_> = ids.iter().map(|id| item(id)).collect();
        serde_json::from_value(json!({"items": items, "nextCursor": next})).unwrap()
    }

    #[test]
    fn every_page_is_read_until_the_last() {
        let mut asked = Vec::new();
        let found = all_pages(|request| {
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
        let repeated = all_pages(|_| Ok(page(&["a"], Some("same"))));
        assert!(matches!(repeated, Err(Error::InvalidResponse)));
        let mut cursor = 0;
        let long = all_pages(|_| {
            cursor += 1;
            Ok(page(&["a"], Some(cursor.to_string().as_str())))
        })
        .unwrap();
        assert!(long.more);
        assert_eq!(long.items.len(), 10);
        assert!(all_pages(|_| Err(Error::Network)).is_err());
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
