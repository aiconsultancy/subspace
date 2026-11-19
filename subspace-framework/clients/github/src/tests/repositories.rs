//! Repository-scoped sections and the repository search, offline.

use super::{Handler, Request, client, pr, serve, token};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// Records every search query; answers one PR per search, numbered by call.
fn recording() -> (Arc<Handler>, Arc<Mutex<Vec<String>>>) {
    let queries = Arc::new(Mutex::new(Vec::new()));
    let seen = queries.clone();
    let handler: Arc<Handler> = Arc::new(move |request: &Request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let query = body["variables"]["query"].as_str().unwrap().to_owned();
        let document = body["query"].as_str().unwrap();
        if document.contains("isDraft") {
            seen.lock().unwrap().push(query.clone());
        }
        let number = query.len() as u32;
        let nodes = if document.contains("isDraft") {
            json!([pr(&format!("N{number}"), number)])
        } else {
            json!([])
        };
        let answer = json!({"data": {"search": {"issueCount": 3, "nodes": nodes}}});
        (200, answer.to_string())
    });
    (handler, queries)
}

#[test]
fn all_open_searches_only_the_given_repositories() {
    let (handler, queries) = recording();
    let client = client(serve(handler), &token("tok"));
    let repos = ["acme/web".to_owned(), "acme/api".to_owned()];
    let section = client.section("ann", "open", Some(&repos)).unwrap();
    assert_eq!(
        *queries.lock().unwrap(),
        ["is:pr is:open archived:false repo:acme/web repo:acme/api sort:updated-desc"]
    );
    assert_eq!(section.section, "open");
    assert_eq!(section.total, 3);
    assert_eq!(section.items.len(), 1);
}

#[test]
fn all_open_without_repositories_is_empty_and_sends_nothing() {
    let (handler, queries) = recording();
    let client = client(serve(handler), &token("tok"));
    for repos in [None, Some(&[][..])] {
        let section = client.section("ann", "open", repos).unwrap();
        assert_eq!((section.total, section.items.len()), (0, 0));
    }
    assert!(queries.lock().unwrap().is_empty());
}

#[test]
fn a_personal_section_can_be_limited_to_repositories_in_chunks() {
    let (handler, queries) = recording();
    let client = client(serve(handler), &token("tok"));
    let repos: Vec<String> = (0..30).map(|n| format!("acme/service-{n:02}")).collect();
    let section = client.section("ann", "mine", Some(&repos)).unwrap();
    let queries = queries.lock().unwrap().clone();
    assert!(queries.len() > 1, "{queries:?}");
    assert!(queries.iter().all(|query| query.contains("author:@me")));
    assert!(queries.iter().all(|query| query.len() <= 256));
    assert_eq!(section.total, 3 * queries.len() as u32);
    let account_wide = client.section("ann", "mine", None).unwrap();
    assert_eq!(account_wide.total, 3);
}

#[test]
fn repository_search_uses_github_search_or_the_accounts_own_repositories() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let seen = lines.clone();
    let handler: Arc<Handler> = Arc::new(move |request: &Request| {
        seen.lock().unwrap().push(request.line.clone());
        let repo = json!({"full_name": "acme/web", "description": null, "private": true});
        if request.line.starts_with("GET /search/repositories") {
            (200, json!({"total_count": 1, "items": [repo]}).to_string())
        } else {
            (200, json!([repo, {"full_name": "acme/api"}]).to_string())
        }
    });
    let client = client(serve(handler), &token("tok"));
    let found = client.repositories("ann", " web ui ").unwrap();
    assert_eq!(found[0].full_name, "acme/web");
    assert!(found[0].private);
    let own = client.repositories("ann", "").unwrap();
    assert_eq!(own.len(), 2);
    assert!(!own[1].archived);
    let lines = lines.lock().unwrap();
    assert!(lines[0].starts_with("GET /search/repositories?q=web+ui&per_page=30 "));
    assert!(lines[1].starts_with("GET /user/repos?sort=pushed&per_page=30&affiliation="));
    assert!(client.repositories("ann", "a\u{7}").is_err());
}
