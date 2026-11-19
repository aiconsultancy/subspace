//! Searches limited to named repositories. Each repository becomes one
//! `repo:` qualifier, which GitHub combines with OR, and the qualifiers are
//! split across as many searches as GitHub's query length limit needs.

use subspace_core::http::Error;

use crate::model::GhItem;
use crate::validate::{check_repo, invalid};

/// GitHub's longest search query.
pub const MAX_QUERY_CHARS: usize = 256;
/// A section shows at most this many items, as one GraphQL search page.
pub const SECTION_ITEMS: usize = 50;
const SORT: &str = "sort:updated-desc";
/// The longest repository search text, as GitHub accepts it.
const MAX_SEARCH_TEXT_CHARS: usize = 256;

/// The searches that make up one section: one account-wide search without
/// `repos`; with `repos`, one search per chunk of `repo:` qualifiers, and
/// none for an empty list. Every name must be `owner/name`.
pub fn searches(base: &str, repos: Option<&[String]>) -> Result<Vec<String>, Error> {
    match repos {
        None => Ok(vec![query(base, &[])]),
        Some(repos) => chunks(base, &qualifiers(repos)?),
    }
}

/// `repo:owner/name` for each valid name, once each without regard to case.
fn qualifiers(repos: &[String]) -> Result<Vec<String>, Error> {
    let mut seen: Vec<String> = Vec::new();
    for repo in repos {
        check_repo(repo)?;
        if !seen.iter().any(|known| known.eq_ignore_ascii_case(repo)) {
            seen.push(repo.clone());
        }
    }
    Ok(seen
        .into_iter()
        .map(|repo| format!("repo:{repo}"))
        .collect())
}

fn query(base: &str, qualifiers: &[String]) -> String {
    let mut parts = vec![base];
    parts.extend(qualifiers.iter().map(String::as_str));
    parts.push(SORT);
    parts.join(" ")
}

/// Packs `qualifiers` in order into queries of at most [`MAX_QUERY_CHARS`].
fn chunks(base: &str, qualifiers: &[String]) -> Result<Vec<String>, Error> {
    let empty = query(base, &[]).len();
    let mut queries = Vec::new();
    let mut chunk: Vec<String> = Vec::new();
    let mut length = empty;
    for qualifier in qualifiers {
        let added = qualifier.len() + 1;
        if empty + added > MAX_QUERY_CHARS {
            return Err(invalid("A repository name is too long to search."));
        }
        if length + added > MAX_QUERY_CHARS {
            queries.push(query(base, &chunk));
            chunk.clear();
            length = empty;
        }
        chunk.push(qualifier.clone());
        length += added;
    }
    if !chunk.is_empty() {
        queries.push(query(base, &chunk));
    }
    Ok(queries)
}

/// One section from the answers of its searches: their counts added up, and
/// the most recently updated items, each once.
pub fn merge(results: Vec<(u32, Vec<GhItem>)>) -> (u32, Vec<GhItem>) {
    let total = results.iter().map(|(count, _)| *count).sum();
    let mut items: Vec<GhItem> = Vec::new();
    for item in results.into_iter().flat_map(|(_, items)| items) {
        if !items.iter().any(|known| known.key == item.key) {
            items.push(item);
        }
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    items.truncate(SECTION_ITEMS);
    (total, items)
}

/// Repository search text: at most 256 characters, no control characters.
pub fn check_search_text(text: &str) -> Result<&str, Error> {
    let text = text.trim();
    if text.chars().count() > MAX_SEARCH_TEXT_CHARS || text.chars().any(char::is_control) {
        return Err(invalid(
            "Search text must be at most 256 characters, without control characters.",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{MAX_QUERY_CHARS, check_search_text, merge, searches};
    use crate::model::{GhItem, ItemKind, PrState};

    fn names(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| format!("acme/repo-{index:02}"))
            .collect()
    }

    #[test]
    fn an_account_wide_search_has_no_repository_qualifiers() {
        assert_eq!(
            searches("is:pr is:open author:@me", None).unwrap(),
            ["is:pr is:open author:@me sort:updated-desc"]
        );
    }

    #[test]
    fn repositories_become_repo_qualifiers_once_each() {
        let repos = ["acme/web".to_owned(), "ACME/Web".into(), "acme/api".into()];
        assert_eq!(
            searches("is:pr is:open", Some(&repos)).unwrap(),
            ["is:pr is:open repo:acme/web repo:acme/api sort:updated-desc"]
        );
    }

    #[test]
    fn no_repositories_means_no_search() {
        assert!(searches("is:pr is:open", Some(&[])).unwrap().is_empty());
    }

    #[test]
    fn long_lists_split_into_queries_within_the_limit_in_order() {
        let repos = names(30);
        let queries = searches("is:pr is:open archived:false", Some(&repos)).unwrap();
        assert!(queries.len() > 1);
        assert!(queries.iter().all(|query| query.len() <= MAX_QUERY_CHARS));
        assert!(
            queries
                .iter()
                .all(|query| query.starts_with("is:pr is:open"))
        );
        assert!(
            queries
                .iter()
                .all(|query| query.ends_with("sort:updated-desc"))
        );
        let found: Vec<&str> = queries
            .iter()
            .flat_map(|query| query.split(' '))
            .filter_map(|word| word.strip_prefix("repo:"))
            .collect();
        assert_eq!(found, repos);
    }

    #[test]
    fn names_that_could_change_the_query_are_refused() {
        for bad in [
            "acme/web repo:other/x",
            "acme/web\"",
            "acme",
            "acme/web/x",
            "acme/web OR is:issue",
            "",
        ] {
            assert!(
                searches("is:pr", Some(&[bad.to_owned()])).is_err(),
                "{bad:?}"
            );
        }
    }

    fn item(key: &str, updated: &str) -> GhItem {
        let (repo, number) = key.split_once('#').unwrap();
        GhItem {
            id: key.into(),
            key: key.into(),
            kind: ItemKind::Pr,
            repo: repo.into(),
            number: number.parse().unwrap(),
            title: key.into(),
            url: String::new(),
            state: PrState::Open,
            author: None,
            assignees: Vec::new(),
            labels: Vec::new(),
            comments: 0,
            created_at: updated.into(),
            updated_at: updated.into(),
            review_decision: None,
            checks: None,
            additions: None,
            deletions: None,
            head_ref: None,
            base_ref: None,
        }
    }

    #[test]
    fn chunk_answers_merge_newest_first_and_add_their_counts() {
        let first = vec![item("a/x#1", "2026-01-03"), item("a/x#2", "2026-01-01")];
        let second = vec![item("b/y#7", "2026-01-02"), item("a/x#1", "2026-01-03")];
        let (total, items) = merge(vec![(2, first), (60, second)]);
        assert_eq!(total, 62);
        let keys: Vec<&str> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["a/x#1", "b/y#7", "a/x#2"]);
        let many = (0..80)
            .map(|n| item(&format!("a/x#{}", n + 1), "2026-01-01"))
            .collect();
        assert_eq!(merge(vec![(80, many)]).1.len(), super::SECTION_ITEMS);
    }

    #[test]
    fn search_text_is_bounded_and_trimmed() {
        assert_eq!(check_search_text("  indra ").unwrap(), "indra");
        assert!(check_search_text(&"x".repeat(257)).is_err());
        assert!(check_search_text("a\nb").is_err());
    }
}
