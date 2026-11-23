//! The synchronous GitHub client: accounts, sections, detail, writes and pull
//! request diffs. No GPUI; dashboards call it off the UI thread.
//!
//! Every call runs as one account. `login` is a signed-in or gh login from
//! [`Client::accounts`]; with `GH_TOKEN`, `GITHUB_TOKEN` or `github.token`
//! set, that token is used whatever the login (REQ-60).

pub mod actions;
mod bound;
pub mod copilot_agents;
pub use bound::{
    BoundRead, Organization, PackageRelease, PackageRepository, ReleaseAsset, RepositoryRef,
    ResolvedRepository, newest_package_release,
};
mod cache;
pub mod copilot_agent_sessions;
pub mod issue_report;
pub mod lifecycle;
pub mod model;
mod pull_request_writes;
mod queries;
mod raw;
pub mod scope;
mod transport;
pub mod validate;
mod work;
pub use transport::is_access_refusal;
pub use work::PinnedWork;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use subspace_core::credentials::{
    Credential, CredentialError, Credentials, GitHubAccounts, Source,
};
use subspace_core::http::status as http_status;
use subspace_core::http::{Error, send_with_retry};

use crate::cache::{Lru, Ttl};
use crate::model::{
    AssignableResponse, DiffResponse, FileResponse, GhDiffFile, GhItemDetail, GhRepository, GhUser,
    Image, ItemKind, ReviewEvent, SECTIONS, SectionResponse, image_type,
};
use crate::raw::{Nodes, RawActor, RawDetail, RawPrFile};
use crate::transport::{JSON, RAW, not_found, unexpected};
use crate::validate::{
    ItemRef, check_key, check_login, check_logins, check_path, check_repo, check_review_body,
    check_section, check_sha, invalid, is_sha,
};

pub const API_URL: &str = "https://api.github.com";
/// Debug builds only: the GitHub API and Copilot API base to use instead of
/// [`API_URL`], such as a local fixture server for screenshots.
pub const API_URL_ENV: &str = "SUBSPACE_GITHUB_API_URL";
const USER_RESPONSE_LIMIT: usize = 64 * 1024;
/// The read limit for a JSON answer that can reach a few megabytes.
pub(crate) const MAX_BODY_BYTES: usize = subspace_core::http::MAX_BODY_BYTES as usize;

/// The login that owns `credential`, from `GET /user`. No retry, cache or
/// fallback credential: the answer identifies exactly this token.
pub fn credential_login(credential: &Credential) -> Result<String, Error> {
    #[derive(Deserialize)]
    struct User {
        login: String,
    }
    let account = transport::account_label(credential, None);
    let url = format!("{}/user", api_url());
    let response = transport::get_rest(&url, credential, &account, JSON)?;
    let bytes = transport::read_limited(response, USER_RESPONSE_LIMIT)?;
    let user: User = serde_json::from_slice(&bytes).map_err(|_| unexpected("user"))?;
    validate::check_login(&user.login).map(str::to_owned)
}

/// The [`API_URL_ENV`] override, in a debug build when it is set.
fn debug_api_url() -> Option<String> {
    let value = std::env::var(API_URL_ENV)
        .ok()
        .filter(|_| cfg!(debug_assertions))?;
    let value = value.trim().trim_end_matches('/');
    (!value.is_empty()).then(|| value.to_owned())
}

/// [`API_URL`], unless a debug build overrides it.
fn api_url() -> String {
    debug_api_url().unwrap_or_else(|| API_URL.to_owned())
}

/// The Copilot API base, known up front only when a debug build overrides
/// the API URL; otherwise GraphQL names it on first use.
fn copilot_api_override() -> std::sync::OnceLock<String> {
    debug_api_url()
        .map(std::sync::OnceLock::from)
        .unwrap_or_default()
}
const ASSIGNABLE_TTL: Duration = Duration::from_secs(10 * 60);
/// GitHub's limit for the pulls files API.
pub const MAX_DIFF_FILES: u32 = 3000;
const FILES_PER_PAGE: u32 = 100;
const PAGE_CONCURRENCY: u32 = 6;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const FILE_CACHE_BYTES: usize = 50 * 1024 * 1024;
const DIFF_CACHE_ENTRIES: usize = 20;
const MAX_AGENT_SEARCH_BYTES: usize = 200;
const MAX_AGENT_SEARCH_ITEMS: u32 = 100;
const REPOSITORIES_PER_PAGE: &str = "30";

/// What one diff read needs: the account, the pull request, its commits
/// and how many file pages to read.
struct DiffRequest<'a> {
    login: &'a str,
    item: &'a ItemRef,
    base_tip: &'a str,
    head: &'a str,
    pages: u32,
}

/// The head and merge-base commits of a pull request.
fn pull_commits(pull: &RawPull) -> Result<(String, String), Error> {
    let sha = |side: &Option<Sha>| {
        side.as_ref()
            .and_then(|side| side.sha.clone())
            .unwrap_or_default()
    };
    let head = sha(&pull.head);
    let base_tip = sha(&pull.base);
    if !is_sha(&head) || !is_sha(&base_tip) {
        return Err(Error::Status {
            status: http_status::BAD_GATEWAY,
            message: "GitHub returned no commits for this pull request.".into(),
        });
    }
    Ok((head, base_tip))
}

// qual:allow(srp, god_struct) reason: "The bound-read delegate shares this account-scoped client but is counted as an independent cluster."
pub struct Client {
    credentials: Arc<Credentials>,
    work_account: Option<String>,
    api_url: String,
    /// The Copilot API base GraphQL named, resolved at most once per run.
    copilot_api: std::sync::OnceLock<String>,
    /// Whose the last work credential [`Client::pin_work`] verified is.
    verified: std::sync::Mutex<Option<work::Verified>>,
    assignable: Ttl<Vec<GhUser>>,
    diffs: Lru<DiffResponse>,
    files: Lru<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Search {
    #[serde(default)]
    issue_count: u32,
    #[serde(default)]
    nodes: Vec<Value>,
}

#[derive(Deserialize)]
struct SearchData {
    search: Search,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository<T> {
    issue_or_pull_request: Option<T>,
}

#[derive(Deserialize)]
struct RepositoryData<T> {
    repository: Option<Repository<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Subject {
    #[serde(rename = "__typename")]
    typename: String,
    id: String,
}

#[derive(Deserialize)]
struct Sha {
    sha: Option<String>,
}

#[derive(Deserialize)]
struct RawPull {
    head: Option<Sha>,
    base: Option<Sha>,
    changed_files: Option<u32>,
}

#[derive(Deserialize)]
struct RawCompare {
    merge_base_commit: Option<Sha>,
}

fn valid_agent_search(query: &str, limit: u32) -> bool {
    let lower = query.to_ascii_lowercase();
    query.len() <= MAX_AGENT_SEARCH_BYTES
        && (1..=MAX_AGENT_SEARCH_ITEMS).contains(&limit)
        && !["repo:", "org:", "user:"]
            .iter()
            .any(|scope| lower.contains(scope))
        && !query.chars().any(char::is_control)
        && !query.contains(['(', ')'])
        && !query
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("OR"))
}

fn agent_mutation(action: &str, kind: &str) -> Result<(&'static str, &'static str), &'static str> {
    match (action, kind) {
        ("comment", _) => Ok((
            "mutation($input:AddCommentInput!) { addComment(input:$input) { commentEdge { node { id url } } } }",
            "subjectId",
        )),
        ("review", "PullRequest") => Ok((
            "mutation($input:AddPullRequestReviewInput!) { addPullRequestReview(input:$input) { pullRequestReview { id url } } }",
            "pullRequestId",
        )),
        ("update_pr", "PullRequest") => Ok((
            "mutation($input:UpdatePullRequestInput!) { updatePullRequest(input:$input) { pullRequest { id url title state } } }",
            "pullRequestId",
        )),
        _ => Err("Unsupported action or target is not a pull request."),
    }
}

fn agent_mutation_body(
    mutation: (&str, &str),
    subject: &str,
    mut fields: Value,
) -> Result<String, &'static str> {
    fields
        .as_object_mut()
        .ok_or("Mutation fields must be an object.")?
        .insert(mutation.1.into(), json!(subject));
    Ok(json!({"query":mutation.0,"variables":{"input":fields}}).to_string())
}

#[cfg(test)]
mod agent_tools_tests {
    use super::{agent_mutation, agent_mutation_body, valid_agent_search};
    use serde_json::{Value, json};
    #[test]
    fn search_cannot_override_repo_scope_or_bounds() {
        assert!(valid_agent_search("is:pr merged:>=2026-10-01", 100));
        for query in [
            "repo:outside/repo",
            "org:outside",
            "user:outside",
            "is:pr OR is:issue",
            "(is:pr)",
            "is:pr\n",
        ] {
            assert!(!valid_agent_search(query, 50), "{query}");
        }
        assert!(!valid_agent_search("is:pr", 101));
        assert!(!valid_agent_search("is:pr", 0));
    }
    #[test]
    fn writes_use_only_named_mutations_and_verified_subject() {
        assert!(agent_mutation("review", "Issue").is_err());
        assert!(agent_mutation("merge", "PullRequest").is_err());
        let mutation = agent_mutation("comment", "Issue").unwrap();
        let body = agent_mutation_body(
            mutation,
            "verified-id",
            json!({"subjectId":"untrusted-id","body":"exact comment"}),
        )
        .unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["variables"]["input"]["subjectId"], "verified-id");
        assert_eq!(body["variables"]["input"]["body"], "exact comment");
        assert!(agent_mutation_body(mutation, "id", json!("not an object")).is_err());
        assert_eq!(
            agent_mutation("review", "PullRequest").unwrap().1,
            "pullRequestId"
        );
        assert_eq!(
            agent_mutation("update_pr", "PullRequest").unwrap().1,
            "pullRequestId"
        );
    }
}

impl Client {
    /// Capture one credential and verify that it owns the requested login.
    // qual:api
    pub fn bind_read(&self, login: &str) -> Result<BoundRead, Error> {
        BoundRead::bind(self, login)
    }

    pub fn new(credentials: Arc<Credentials>) -> Self {
        Self {
            credentials,
            work_account: None,
            api_url: api_url(),
            copilot_api: copilot_api_override(),
            verified: std::sync::Mutex::new(None),
            assignable: Ttl::new(ASSIGNABLE_TTL),
            diffs: Lru::new(DIFF_CACHE_ENTRIES, |_| 1),
            files: Lru::new(FILE_CACHE_BYTES, String::len),
        }
    }

    /// One bounded repository search. Scope is part of the GraphQL repository,
    /// so user search text cannot widen it to other repositories.
    // qual:api
    pub fn agent_search(
        &self,
        login: &str,
        repo: &str,
        query: &str,
        limit: u32,
    ) -> Result<Value, Error> {
        check_login(login)?;
        check_repo(repo)?;
        valid_agent_search(query, limit).then_some(()).ok_or_else(|| invalid("Use at most 200 characters of search terms without scope qualifiers, OR, parentheses or control characters; limit is 1..100."))?;
        self.graphql_value(Some(login), "query($query:String!, $limit:Int!) { search(query:$query,type:ISSUE,first:$limit) { issueCount pageInfo { hasNextPage endCursor } nodes { ... on Issue { number title body state url updatedAt repository { nameWithOwner } author { login } } ... on PullRequest { number title body state url updatedAt mergedAt repository { nameWithOwner } author { login } } } } }", json!({"query":format!("repo:{repo} {query}"),"limit":limit}), false)
    }

    /// Named GraphQL writes, each sent once. Never offers a raw HTTP surface.
    // qual:api
    pub fn agent_write(
        &self,
        login: &str,
        key: &str,
        action: &str,
        fields: Value,
    ) -> Result<Value, Error> {
        check_login(login)?;
        let item = check_key(key)?;
        let subject = self.subject(login, &item)?;
        let mutation = agent_mutation(action, &subject.typename).map_err(invalid)?;
        let body = agent_mutation_body(mutation, &subject.id, fields).map_err(invalid)?;
        let credential = self.credentials.github(Some(login))?;
        let account = transport::account_label(&credential, Some(login));
        let response = transport::post_graphql(
            &format!("{}/graphql", self.api_url),
            &credential,
            &account,
            &body,
        )?;
        transport::graphql_data(&account, response, false)
    }

    /// Sends every request to `api_url` instead of api.github.com (tests).
    pub fn with_api_url(mut self, api_url: impl Into<String>) -> Self {
        self.api_url = api_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// Prefer this configured gh account for Copilot work, before EMU detection.
    // qual:api
    pub fn with_work_account(mut self, login: Option<String>) -> Self {
        self.work_account = login;
        self
    }

    /// The accounts to offer: the signed-in and gh accounts for github.com;
    /// but when the token comes from the environment or the config file, the
    /// one account that token belongs to (REQ-60).
    pub fn accounts(&self) -> Result<GitHubAccounts, Error> {
        let (hosts, credential) = match self.credentials.github_accounts() {
            Ok(hosts) => {
                let login = hosts
                    .active
                    .clone()
                    .or_else(|| hosts.logins.first().cloned());
                let credential = self.credentials.github(login.as_deref())?;
                (Some(hosts), credential)
            }
            Err(_) => (None, self.credentials.github(None)?),
        };
        if let (Some(hosts), Source::GhCli { .. } | Source::SignedIn { .. }) =
            (hosts, &credential.source)
        {
            return Ok(hosts);
        }
        let login = self.viewer_login(None)?;
        Ok(GitHubAccounts {
            logins: vec![login.clone()],
            active: Some(login),
            signed_in: Vec::new(),
        })
    }

    /// The login of the token for `login` (or the default token).
    pub fn viewer_login(&self, login: Option<&str>) -> Result<String, Error> {
        #[derive(Deserialize)]
        struct Viewer {
            login: String,
        }
        #[derive(Deserialize)]
        struct Data {
            viewer: Viewer,
        }
        let data: Data = self.graphql(login, queries::VIEWER_QUERY, json!({}), false)?;
        Ok(data.viewer.login)
    }

    /// Resolves the work account's credential once and verifies whose it is
    /// with that same credential, for an operation whose every request must
    /// be made as one account. The chosen gh account wins, else the first
    /// enterprise (EMU) login, as in [`Client::work_login`]. A credential
    /// verified earlier this run is not looked up again, so this needs no
    /// request then, offline included.
    pub fn pin_work(&self) -> Result<PinnedWork<'_>, Error> {
        PinnedWork::pin(self)
    }

    /// Selects the chosen gh account, else the first enterprise (EMU) login,
    /// and verifies that the selected credential really belongs to it.
    /// Environment tokens belonging to another account are rejected. A
    /// caller that goes on to make requests as that account should use
    /// [`Client::pin_work`] instead, so they go out with the credential
    /// verified here.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn work_login(&self) -> Result<String, Error> {
        self.pin_work().map(|work| work.login().to_owned())
    }

    /// The credential for `login` (REQ-21), for callers that only need to
    /// know whether one exists.
    pub fn credential(&self, login: Option<&str>) -> Result<Credential, CredentialError> {
        self.credentials.github(login)
    }

    fn graphql_value(
        &self,
        login: Option<&str>,
        query: &str,
        variables: Value,
        partial: bool,
    ) -> Result<Value, Error> {
        let url = format!("{}/graphql", self.api_url);
        let body = json!({ "query": query, "variables": variables }).to_string();
        send_with_retry(
            &self.credentials,
            |credentials| credentials.github(login),
            |credential| {
                let account = transport::account_label(credential, login);
                let response = transport::post_graphql(&url, credential, &account, &body)?;
                transport::graphql_data(&account, response, partial)
            },
        )
    }

    fn graphql<T: DeserializeOwned>(
        &self,
        login: Option<&str>,
        query: &str,
        variables: Value,
        partial: bool,
    ) -> Result<T, Error> {
        let data = self.graphql_value(login, query, variables, partial)?;
        serde_json::from_value(data).map_err(|_| unexpected("response"))
    }

    /// A REST URL: `segments` are percent-encoded one by one.
    fn rest_url<'a>(
        &self,
        segments: impl IntoIterator<Item = &'a str>,
        query: &[(&str, &str)],
    ) -> Result<String, Error> {
        let mut url = url::Url::parse(&self.api_url)
            .map_err(|_| invalid("The GitHub API URL is not valid."))?;
        url.path_segments_mut()
            .map_err(|()| invalid("The GitHub API URL is not valid."))?
            .pop_if_empty()
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url.into())
    }

    fn rest_bytes(
        &self,
        login: &str,
        url: &str,
        accept: &str,
        max: usize,
    ) -> Result<Vec<u8>, Error> {
        send_with_retry(
            &self.credentials,
            |credentials| credentials.github(Some(login)),
            |credential| {
                let account = transport::account_label(credential, Some(login));
                let response = transport::get_rest(url, credential, &account, accept)?;
                transport::read_limited(response, max)
            },
        )
    }

    fn rest_json<T: DeserializeOwned>(&self, login: &str, url: &str) -> Result<T, Error> {
        // JSON answers of the pulls files API reach a few MB with patches.
        let bytes = self.rest_bytes(login, url, JSON, MAX_BODY_BYTES)?;
        serde_json::from_slice(&bytes).map_err(|_| unexpected("response"))
    }

    /// One section: up to 50 items, most recently updated first. `repos`
    /// limits it to those repositories (`None`: everything the account can
    /// see); a [`repos_only`](model::SectionDef::repos_only) section without
    /// repositories is empty and sends nothing. Long repository lists run as
    /// several searches in parallel, merged by update time.
    pub fn section(
        &self,
        login: &str,
        section_id: &str,
        repos: Option<&[String]>,
    ) -> Result<SectionResponse, Error> {
        check_login(login)?;
        let section = check_section(section_id)?;
        let repos = if section.repos_only {
            Some(repos.unwrap_or_default())
        } else {
            repos
        };
        let searches = scope::searches(section.query, repos)?;
        let results: Vec<Result<(u32, Vec<model::GhItem>), Error>> = thread::scope(|scope| {
            let handles: Vec<_> = searches
                .iter()
                .map(|query| scope.spawn(|| self.section_search(login, section.kind, query)))
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(Error::Network("The request failed.".into())))
                })
                .collect()
        });
        let (total, items) = scope::merge(results.into_iter().collect::<Result<_, _>>()?);
        Ok(SectionResponse {
            account: login.to_owned(),
            section: section.id.to_owned(),
            total,
            items,
        })
    }

    /// One search of a section. Populated PR searches run two optional
    /// enrichment searches in parallel after the row query. Empty or failed
    /// row queries do not launch enrichment requests.
    fn section_search(
        &self,
        login: &str,
        kind: ItemKind,
        query: &str,
    ) -> Result<(u32, Vec<model::GhItem>), Error> {
        let variables = json!({ "query": query });
        // partial: items in SSO-protected organisations come back as null.
        let search =
            |query: &str| self.graphql::<SearchData>(Some(login), query, variables.clone(), true);
        let main_query = queries::section_query();
        let main = search(&main_query)?.search;
        let extra_queries =
            if kind == ItemKind::Pr && !raw::section_items(&main.nodes, &[]).is_empty() {
                queries::pr_extra_queries().to_vec()
            } else {
                Vec::new()
            };
        let extras: Vec<SearchData> = thread::scope(|scope| {
            let handles: Vec<_> = extra_queries
                .iter()
                .map(|query| scope.spawn(|| search(query)))
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok()?.ok())
                .collect()
        });
        let extra_nodes: Vec<Value> = extras
            .into_iter()
            .flat_map(|extra| extra.search.nodes)
            .collect();
        let items = raw::section_items(&main.nodes, &raw::extras(&extra_nodes));
        Ok((main.issue_count, items))
    }

    /// Repositories the account can see for `text`: GitHub's repository
    /// search, or with no text the account's own and member repositories,
    /// most recently pushed first. At most 30.
    pub fn repositories(&self, login: &str, text: &str) -> Result<Vec<GhRepository>, Error> {
        #[derive(Deserialize)]
        struct Found {
            items: Vec<GhRepository>,
        }
        check_login(login)?;
        let text = scope::check_search_text(text)?;
        if text.is_empty() {
            let url = self.rest_url(
                ["user", "repos"],
                &[
                    ("sort", "pushed"),
                    ("per_page", REPOSITORIES_PER_PAGE),
                    ("affiliation", "owner,collaborator,organization_member"),
                ],
            )?;
            return self.rest_json(login, &url);
        }
        let url = self.rest_url(
            ["search", "repositories"],
            &[("q", text), ("per_page", REPOSITORIES_PER_PAGE)],
        )?;
        Ok(self.rest_json::<Found>(login, &url)?.items)
    }

    /// Every section, account-wide, in [`SECTIONS`] order, in parallel.
    pub fn sections(&self, login: &str) -> Vec<(&'static str, Result<SectionResponse, Error>)> {
        thread::scope(|scope| {
            let handles: Vec<_> = SECTIONS
                .iter()
                .map(|section| {
                    (
                        section.id,
                        scope.spawn(|| self.section(login, section.id, None)),
                    )
                })
                .collect();
            handles
                .into_iter()
                .map(|(id, handle)| {
                    let result = handle
                        .join()
                        .unwrap_or_else(|_| Err(Error::Network("The request failed.".into())));
                    (id, result)
                })
                .collect()
        })
    }

    fn load_detail(&self, login: &str, item: &ItemRef) -> Result<GhItemDetail, Error> {
        let variables = json!({ "owner": item.owner, "name": item.name, "number": item.number });
        let data: RepositoryData<RawDetail> =
            self.graphql(Some(login), &queries::detail_query(), variables, false)?;
        detail_from_data(data)
    }

    /// The detail of `key` (`owner/name#number`).
    pub fn item(&self, login: &str, key: &str) -> Result<GhItemDetail, Error> {
        check_login(login)?;
        self.load_detail(login, &check_key(key)?)
    }

    /// The people who can be assigned in `repo`, cached for 10 minutes.
    // qual:api
    pub fn assignable(&self, login: &str, repo: &str) -> Result<AssignableResponse, Error> {
        check_login(login)?;
        let (owner, name) = check_repo(repo)?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Assignable {
            assignable_users: Nodes<RawActor>,
        }
        #[derive(Deserialize)]
        struct Data {
            repository: Option<Assignable>,
        }
        let key = format!("gh-assignable:{login}:{owner}/{name}").to_lowercase();
        let users = self.assignable.get_or_load(&key, || {
            let variables = json!({ "owner": owner, "name": name });
            let data: Data =
                self.graphql(Some(login), queries::ASSIGNABLE_QUERY, variables, false)?;
            let repository = data.repository.ok_or_else(|| Error::Status {
                status: http_status::NOT_FOUND,
                message: "Repository not found on GitHub.".into(),
            })?;
            Ok::<_, Error>(raw::assignable_users(&repository.assignable_users))
        })?;
        Ok(AssignableResponse { users })
    }

    fn subject(&self, login: &str, item: &ItemRef) -> Result<Subject, Error> {
        let variables = json!({ "owner": item.owner, "name": item.name, "number": item.number });
        let data: RepositoryData<Subject> =
            self.graphql(Some(login), queries::SUBJECT_QUERY, variables, false)?;
        data.repository
            .and_then(|repository| repository.issue_or_pull_request)
            .ok_or_else(not_found)
    }

    /// Approves, requests changes or comments as a review; returns the
    /// fresh detail. The body is required except for an approval.
    pub fn review(
        &self,
        login: &str,
        key: &str,
        event: ReviewEvent,
        body: &str,
    ) -> Result<GhItemDetail, Error> {
        check_login(login)?;
        let item = check_key(key)?;
        let body = check_review_body(event, body)?;
        let subject = self.subject(login, &item)?;
        if subject.typename != "PullRequest" {
            return Err(invalid("Reviews apply only to pull requests."));
        }
        let body = (!body.is_empty()).then_some(body);
        let variables = json!({ "id": subject.id, "event": event.as_str(), "body": body });
        self.graphql_value(Some(login), queries::REVIEW_MUTATION, variables, false)?;
        self.load_detail(login, &item)
    }

    /// The repository's default branch, which a cloud agent task branches
    /// from when the caller names none.
    pub fn default_branch(&self, login: &str, repo: &str) -> Result<String, Error> {
        check_login(login)?;
        check_repo(repo)?;
        let value: Value = self.rest_json(login, &format!("{}/repos/{repo}", self.api_url))?;
        value
            .get("default_branch")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| invalid("The repository reports no default branch."))
    }

    /// Post an issue comment without a refresh or automatic retry.
    pub fn issue_comment(&self, login: &str, key: &str, body: &str) -> Result<(), Error> {
        pull_request_writes::issue_comment(self, login, key, body)
    }

    /// Close a pull request without a refresh or automatic retry.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn close_pull_request(&self, login: &str, key: &str) -> Result<(), Error> {
        pull_request_writes::close(self, login, key)
    }

    /// Comments on an issue or pull request; returns the fresh detail.
    // qual:api
    pub fn comment(&self, login: &str, key: &str, body: &str) -> Result<GhItemDetail, Error> {
        check_login(login)?;
        let item = check_key(key)?;
        let body = validate::check_body(body, true)?;
        let subject = self.subject(login, &item)?;
        let variables = json!({ "id": subject.id, "body": body });
        self.graphql_value(Some(login), queries::COMMENT_MUTATION, variables, false)?;
        self.load_detail(login, &item)
    }

    /// Adds and removes assignees (at most 10 each, at least one in all);
    /// returns the fresh detail.
    // qual:api
    pub fn set_assignees(
        &self,
        login: &str,
        key: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<GhItemDetail, Error> {
        check_login(login)?;
        let item = check_key(key)?;
        let add = check_logins("add", add)?;
        let remove = check_logins("remove", remove)?;
        if add.is_empty() && remove.is_empty() {
            return Err(invalid("add or remove must name at least one login."));
        }
        let mut all: Vec<&String> = Vec::new();
        for user in add.iter().chain(&remove) {
            if !all.contains(&user) {
                all.push(user);
            }
        }
        let mut variables =
            json!({ "owner": item.owner, "name": item.name, "number": item.number });
        for (index, user) in all.iter().enumerate() {
            variables[format!("l{index}")] = json!(user);
        }
        // partial: an unknown login is reported by name, not as GitHub's
        // generic error.
        let data = self.graphql_value(
            Some(login),
            &queries::assignee_lookup_query(all.len()),
            variables,
            true,
        )?;
        let id = data["repository"]["issueOrPullRequest"]["id"]
            .as_str()
            .ok_or_else(not_found)?
            .to_owned();
        let user_id = |user: &String| -> Result<String, Error> {
            let index = all
                .iter()
                .position(|known| *known == user)
                .unwrap_or_default();
            data[format!("u{index}")]["id"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("Unknown GitHub user: {user}.")))
        };
        let add_ids = add.iter().map(user_id).collect::<Result<Vec<_>, _>>()?;
        let remove_ids = remove.iter().map(user_id).collect::<Result<Vec<_>, _>>()?;
        let mut variables = json!({ "id": id });
        if !add_ids.is_empty() {
            variables["add"] = json!(add_ids);
        }
        if !remove_ids.is_empty() {
            variables["remove"] = json!(remove_ids);
        }
        let mutation = queries::assignee_mutation(!add_ids.is_empty(), !remove_ids.is_empty());
        self.graphql_value(Some(login), &mutation, variables, false)?;
        self.load_detail(login, &item)
    }

    /// A pull request's changed files, with the merge base as the old side
    /// of every patch. Cached per base and head SHA.
    pub fn diff(&self, login: &str, key: &str) -> Result<DiffResponse, Error> {
        check_login(login)?;
        let item = check_key(key)?;
        let number = item.number.to_string();
        let repo = item.repo();
        let pull_url = self.rest_url(["repos", &item.owner, &item.name, "pulls", &number], &[])?;
        let pull: RawPull = self.rest_json(login, &pull_url)?;
        let (head, base_tip) = pull_commits(&pull)?;
        let cache_key = format!("{login}\n{repo}#{number}@{base_tip}..{head}");
        if let Some(hit) = self.diffs.get(&cache_key) {
            return Ok(hit);
        }
        let changed_files = pull.changed_files.unwrap_or(0);
        let (base, mut files) = self.diff_files(&DiffRequest {
            login,
            item: &item,
            base_tip: &base_tip,
            head: &head,
            pages: file_pages(changed_files),
        })?;
        files.truncate(MAX_DIFF_FILES as usize);
        let result = DiffResponse {
            repo,
            number: item.number,
            base_sha: base,
            head_sha: head,
            total: pull.changed_files.unwrap_or(files.len() as u32),
            files,
        };
        self.diffs.set(cache_key, result.clone());
        Ok(result)
    }

    /// The merge base and the changed files of one pull request, read in
    /// parallel. Paging stops at the first short page.
    fn diff_files(&self, request: &DiffRequest<'_>) -> Result<(String, Vec<GhDiffFile>), Error> {
        let DiffRequest {
            login,
            item,
            base_tip,
            head,
            pages,
        } = *request;
        let number = item.number.to_string();
        let files_url = |page: u32| {
            let page = page.to_string();
            let per_page = FILES_PER_PAGE.to_string();
            self.rest_url(
                ["repos", &item.owner, &item.name, "pulls", &number, "files"],
                &[("per_page", &per_page), ("page", &page)],
            )
        };
        thread::scope(|scope| {
            let base = scope.spawn(|| self.merge_base(login, item, base_tip, head));
            let mut files: Vec<GhDiffFile> = Vec::new();
            let mut first = 1;
            while first <= pages {
                let last = (first + PAGE_CONCURRENCY - 1).min(pages);
                let handles: Vec<_> = (first..=last)
                    .map(|page| {
                        let url = files_url(page);
                        scope.spawn(move || self.rest_json::<Vec<RawPrFile>>(login, &url?))
                    })
                    .collect();
                let mut short_page = false;
                for handle in handles {
                    let list = handle
                        .join()
                        .unwrap_or_else(|_| Err(Error::Network("The request failed.".into())))?;
                    short_page |= list.len() < FILES_PER_PAGE as usize;
                    files.extend(list.into_iter().map(raw::to_diff_file));
                }
                if short_page {
                    break;
                }
                first = last + 1;
            }
            let base = base.join().unwrap_or_else(|_| base_tip.to_owned());
            Ok((base, files))
        })
    }

    /// The merge base of `base` and `head`, else `base` when the compare
    /// call fails. Page 2 with one commit per page: GitHub lists the
    /// changed files on the first page only.
    fn merge_base(&self, login: &str, item: &ItemRef, base: &str, head: &str) -> String {
        let range = format!("{base}...{head}");
        let compare = self
            .rest_url(
                ["repos", &item.owner, &item.name, "compare", &range],
                &[("per_page", "1"), ("page", "2")],
            )
            .and_then(|url| self.rest_json::<RawCompare>(login, &url));
        compare
            .ok()
            .and_then(|compare| compare.merge_base_commit?.sha)
            .filter(|sha| is_sha(sha))
            .unwrap_or_else(|| base.to_owned())
    }

    fn contents_url(
        &self,
        owner: &str,
        name: &str,
        sha: &str,
        path: &str,
    ) -> Result<String, Error> {
        let segments = ["repos", owner, name, "contents"]
            .into_iter()
            .chain(path.split('/'));
        self.rest_url(segments, &[("ref", sha)])
    }

    /// One file's text at a commit, up to 1 MB (else 413); a binary file is
    /// a 415. Cached per account, up to 50 MB.
    pub fn file(
        &self,
        login: &str,
        repo: &str,
        sha: &str,
        path: &str,
    ) -> Result<FileResponse, Error> {
        check_login(login)?;
        let (owner, name) = check_repo(repo)?;
        let sha = check_sha(sha)?;
        let path = check_path(path)?;
        // The key holds the account: a text must not reach an account
        // without access to it.
        let key = format!("{login}\n{owner}/{name}@{sha}:{path}");
        if let Some(text) = self.files.get(&key) {
            return Ok(FileResponse { text });
        }
        let url = self.contents_url(&owner, &name, sha, path)?;
        let bytes = self.rest_bytes(login, &url, RAW, MAX_FILE_BYTES)?;
        if bytes.contains(&0) {
            return Err(Error::Status {
                status: http_status::UNSUPPORTED_MEDIA_TYPE,
                message: "The file is binary.".into(),
            });
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        self.files.set(key, text.clone());
        Ok(FileResponse { text })
    }

    /// One image at a commit, up to 10 MB. Only the extensions of
    /// [`image_type`]; other paths are a 415. Not cached.
    pub fn image(&self, login: &str, repo: &str, sha: &str, path: &str) -> Result<Image, Error> {
        check_login(login)?;
        let (owner, name) = check_repo(repo)?;
        let sha = check_sha(sha)?;
        let path = check_path(path)?;
        let content_type = image_type(path).ok_or_else(|| Error::Status {
            status: http_status::UNSUPPORTED_MEDIA_TYPE,
            message: "The file is not a supported image.".into(),
        })?;
        let url = self.contents_url(&owner, &name, sha, path)?;
        let bytes = self.rest_bytes(login, &url, RAW, MAX_IMAGE_BYTES)?;
        Ok(Image {
            content_type,
            bytes,
        })
    }
}

fn detail_from_data(data: RepositoryData<RawDetail>) -> Result<GhItemDetail, Error> {
    let node = data
        .repository
        .and_then(|repository| repository.issue_or_pull_request)
        .ok_or_else(not_found)?;
    Ok(raw::to_detail(&node))
}

/// Match a saved account using gh's spelling, else detect the first EMU login.
fn pick_work_login<'a>(logins: &'a [String], chosen: Option<&String>) -> Option<&'a String> {
    chosen
        .and_then(|chosen| {
            logins
                .iter()
                .find(|login| login.eq_ignore_ascii_case(chosen))
        })
        .or_else(|| {
            logins.iter().find(|login| {
                login.rsplit_once('_').is_some_and(|(_, code)| {
                    !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
            })
        })
}

/// A preference never permits a token owned by another account.
fn verified_work_login(login: &str, actual: &str) -> Result<String, Error> {
    if actual != login {
        return Err(Error::Status {
            status: http_status::BAD_GATEWAY,
            message: "The selected work GitHub credential belongs to another account.".into(),
        });
    }
    Ok(login.to_owned())
}

/// Pages of the pulls files API to request: at least one, at most GitHub's
/// 3,000-file limit.
pub fn file_pages(changed_files: u32) -> u32 {
    changed_files
        .clamp(1, MAX_DIFF_FILES)
        .div_ceil(FILES_PER_PAGE)
}

#[cfg(test)]
mod tests;
