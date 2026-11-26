//! Pure parts of the GitHub view: sections, repository groups, ages, initials,
//! check summaries, search documents and the section cache. No GPUI, so the
//! tests run them directly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use subspace_framework::client_github::model::{
    CheckRunState, GhCheck, GhItem, GhItemDetail, GhUser, ItemKind, PrState, SECTIONS, SectionDef,
};
use subspace_framework::dashboard::SearchDoc;

mod written_state;
pub(super) use written_state::preserve_written_state;
mod people;
mod time;

pub use people::user_initials;
pub use time::{age, full_date, now_millis, now_seconds};

/// The section a kind shows first (`DEFAULT_SECTION`).
pub fn default_section(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Pr => "review",
        ItemKind::Issue => "assigned",
    }
}

pub fn section_by_id(id: &str) -> Option<&'static SectionDef> {
    SECTIONS.iter().find(|section| section.id == id)
}

pub fn sections_of(kind: ItemKind) -> impl Iterator<Item = &'static SectionDef> {
    SECTIONS.iter().filter(move |section| section.kind == kind)
}

/// Whether `id` names a pull request section: the board shows pull requests
/// only (REQ-02), so a stored or requested issue section is never entered.
pub fn is_pr_section(id: &str) -> bool {
    section_by_id(id).is_some_and(|section| section.kind == ItemKind::Pr)
}

/// Drops any cached issue section, so a cache saved before the board became
/// PRs-only never resurrects Issues after an upgrade.
pub fn pr_only(sections: HashMap<&'static str, SectionData>) -> HashMap<&'static str, SectionData> {
    sections
        .into_iter()
        .filter(|(id, _)| is_pr_section(id))
        .collect()
}

/// The repositories a section searches: the favourites for a
/// repositories-only section such as All open, and for the other sections
/// while Favourites only is on; else everything the account can see (`None`).
pub fn section_repos(
    section: &SectionDef,
    favourites: &[String],
    favourites_only: bool,
) -> Option<Vec<String>> {
    (section.repos_only || favourites_only).then(|| favourites.to_vec())
}

/// What the board's lists are limited to: whether there are favourite
/// repositories, and whether Favourites only limits the personal sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub has_favourites: bool,
    pub favourites_only: bool,
}

/// Whether `section` is empty only for want of favourites, so its empty
/// state offers to add repositories.
pub fn needs_favourites(section: &str, scope: Scope) -> bool {
    !scope.has_favourites
        && section_by_id(section).is_some_and(|section| section.repos_only || scope.favourites_only)
}

/// The empty state of each section (`EMPTY`), with no favourites to search
/// when All open or Favourites only needs them.
pub fn empty_message(section: &str, scope: Scope) -> (&'static str, &'static str) {
    if needs_favourites(section, scope) {
        let hint = match section {
            "open" => "All open lists the open pull requests of your favourite repositories.",
            _ => "Favourites only is on, so this section shows favourite repositories only.",
        };
        return ("No favourite repositories", hint);
    }
    match section {
        "open" => (
            "No open pull requests",
            "Open pull requests in your favourite repositories show here.",
        ),
        "mine" => ("No open pull requests", "Pull requests you open show here."),
        "review" => (
            "Nothing waiting for your review",
            "Review requests for you and your teams show here.",
        ),
        "involved" => (
            "Nothing else involves you",
            "Open pull requests you comment on, review or are mentioned in show here.",
        ),
        "assigned" => (
            "No issues assigned to you",
            "Open issues assigned to this account show here.",
        ),
        "created" => (
            "No open issues you created",
            "Open issues you open show here.",
        ),
        _ => (
            "No open issues mention you",
            "Open issues that mention this account show here.",
        ),
    }
}

pub fn same_login(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Reader text for an item kind, for the chat context block.
pub fn kind_text(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Pr => "Pull request",
        ItemKind::Issue => "Issue",
    }
}

/// Reader text for an item state, for the chat context block.
pub fn state_text(state: PrState) -> &'static str {
    match state {
        PrState::Open => "Open",
        PrState::Draft => "Draft",
        PrState::Merged => "Merged",
        PrState::Closed => "Closed",
    }
}

/// A branch name, or "None" when the item carries no branch.
pub fn branch_text(branch: Option<&String>) -> &str {
    match branch {
        Some(name) if !name.is_empty() => name,
        _ => "None",
    }
}

// ---- Groups -------------------------------------------------------------------------

/// One repository and its items, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGroup<'a> {
    pub repo: &'a str,
    pub items: Vec<&'a GhItem>,
}

pub fn group_key(repo: &str) -> String {
    format!("repo:{repo}")
}

/// One group per repository, in the order of each repository's most recently
/// updated item (`groupByRepo`).
pub fn group_by_repo(items: &[GhItem]) -> Vec<RepoGroup<'_>> {
    let mut sorted: Vec<&GhItem> = items.iter().collect();
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut groups: Vec<RepoGroup<'_>> = Vec::new();
    for item in sorted {
        match groups.iter_mut().find(|group| group.repo == item.repo) {
            Some(group) => group.items.push(item),
            None => groups.push(RepoGroup {
                repo: &item.repo,
                items: vec![item],
            }),
        }
    }
    groups
}

// ---- Checks -------------------------------------------------------------------------

pub fn is_failed_check(check: &GhCheck) -> bool {
    matches!(
        check.state,
        CheckRunState::Failure | CheckRunState::Cancelled
    )
}

/// "7 checks · 2 failing · 1 pending", or "… · all passed".
pub fn check_summary(checks: &[GhCheck]) -> String {
    let failing = checks.iter().filter(|c| is_failed_check(c)).count();
    let pending = checks
        .iter()
        .filter(|c| c.state == CheckRunState::Pending)
        .count();
    let mut parts = vec![format!(
        "{} {}",
        checks.len(),
        if checks.len() == 1 { "check" } else { "checks" }
    )];
    if failing > 0 {
        parts.push(format!("{failing} failing"));
    }
    if pending > 0 {
        parts.push(format!("{pending} pending"));
    }
    if failing == 0 && pending == 0 && !checks.is_empty() {
        parts.push("all passed".into());
    }
    parts.join(" · ")
}

/// Checks collapse into a summary above this many.
pub const CHECKS_SHOWN: usize = 5;

// ---- Search -------------------------------------------------------------------------

/// Every loaded item of the account once: the first section (in SECTIONS
/// order) that holds an item owns it (`ghDocs`).
pub fn search_items<'a>(
    sections: &'a HashMap<&'static str, SectionData>,
) -> Vec<(&'static str, &'a GhItem)> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for def in SECTIONS {
        let Some(data) = sections.get(def.id) else {
            continue;
        };
        for item in &data.items {
            if seen.contains(&item.key.as_str()) {
                continue;
            }
            seen.push(&item.key);
            out.push((def.id, item));
        }
    }
    out
}

pub fn search_doc(item: &GhItem) -> SearchDoc {
    let mut meta = vec![format!("#{}", item.number)];
    if let Some(author) = &item.author {
        meta.push(author.login.clone());
    }
    meta.extend(item.labels.iter().map(|label| label.name.clone()));
    meta.push(
        serde_json::to_value(item.state)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
    );
    meta.push(
        match item.kind {
            ItemKind::Pr => "pull request",
            ItemKind::Issue => "issue",
        }
        .into(),
    );
    SearchDoc::new(item.key.clone(), item.title.clone())
        .number(u64::from(item.number))
        .parent(item.repo.clone())
        .meta(meta.join(" "))
}

// ---- Section cache ------------------------------------------------------------------

/// One loaded section (`SectionData`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionData {
    pub total: u32,
    pub items: Vec<GhItem>,
    /// Unix milliseconds when loaded.
    pub at: i64,
}

/// The stored sections of one account; malformed sections are dropped
/// (`parseCache`).
pub fn parse_account_cache(value: &serde_json::Value) -> HashMap<&'static str, SectionData> {
    let mut cache = HashMap::new();
    let Some(object) = value.as_object() else {
        return cache;
    };
    for def in SECTIONS {
        let Some(entry) = object.get(def.id) else {
            continue;
        };
        let (Some(total), Some(at), Some(items)) = (
            entry.get("total").and_then(serde_json::Value::as_u64),
            entry.get("at").and_then(serde_json::Value::as_i64),
            entry.get("items").and_then(serde_json::Value::as_array),
        ) else {
            continue;
        };
        let items = items
            .iter()
            .filter_map(|item| serde_json::from_value::<GhItem>(item.clone()).ok())
            .collect();
        cache.insert(
            def.id,
            SectionData {
                total: u32::try_from(total).unwrap_or(u32::MAX),
                items,
                at,
            },
        );
    }
    cache
}

/// Replace an item in every section that holds it (`patchCache`).
pub fn patch_sections(sections: &mut HashMap<&'static str, SectionData>, item: &GhItem) {
    for data in sections.values_mut() {
        for known in &mut data.items {
            if known.key == item.key {
                *known = item.clone();
            }
        }
    }
}

/// The row fields of a detail (`rowOf`).
pub fn row_of(detail: &GhItemDetail) -> GhItem {
    detail.item.clone()
}

/// Whether the reviews composer applies: an open pull request the viewer
/// did not write. Before the detail loads, true (`canReview`).
pub fn can_review(item: &GhItem, detail: Option<&GhItemDetail>) -> bool {
    use subspace_framework::client_github::model::PrState;
    item.kind == ItemKind::Pr
        && matches!(item.state, PrState::Open | PrState::Draft)
        && detail.is_none_or(|detail| detail.viewer_can_review)
}

pub fn can_comment(detail: Option<&GhItemDetail>) -> bool {
    detail.is_none_or(|detail| detail.viewer_can_comment)
}

/// `owner/name#12` → (`owner/name`, 12).
pub fn split_key(key: &str) -> Option<(&str, u32)> {
    let (repo, number) = key.split_once('#')?;
    Some((repo, number.parse().ok()?))
}

/// The assignee menu: current assignees, the account, then the people who
/// can be assigned, each once (`assigneeOptions`).
pub fn assignee_logins(current: &[GhUser], account: &str, assignable: &[GhUser]) -> Vec<GhUser> {
    let mut out: Vec<GhUser> = Vec::new();
    let account_user = GhUser {
        login: account.to_string(),
        name: None,
    };
    for user in current
        .iter()
        .chain(std::iter::once(&account_user))
        .chain(assignable)
    {
        if let Some(known) = out
            .iter_mut()
            .find(|known| same_login(&known.login, &user.login))
        {
            if known.name.is_none() {
                known.name.clone_from(&user.name);
            }
        } else {
            out.push(user.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use subspace_framework::client_github::model::{
        CheckRunState, GhCheck, GhUser, ItemKind, PrState,
    };

    use super::{
        SectionData, assignee_logins, branch_text, check_summary, empty_message, group_by_repo,
        is_pr_section, kind_text, parse_account_cache, patch_sections, pr_only, search_items,
        sections_of, split_key, state_text,
    };
    use crate::sample;

    #[test]
    fn sections_are_filtered_by_kind_in_definition_order() {
        let prs: Vec<&str> = sections_of(ItemKind::Pr)
            .map(|section| section.id)
            .collect();
        let issues: Vec<&str> = sections_of(ItemKind::Issue)
            .map(|section| section.id)
            .collect();

        assert_eq!(prs, ["mine", "review", "involved", "open"]);
        assert_eq!(issues, ["assigned", "created", "mentioned"]);
    }

    const ANY: super::Scope = super::Scope {
        has_favourites: true,
        favourites_only: false,
    };

    #[test]
    fn all_open_searches_the_favourites_and_others_everything_unless_filtered() {
        let favourites = vec!["acme/web".to_owned(), "acme/api".to_owned()];
        let open = super::section_by_id("open").unwrap();
        for only in [false, true] {
            assert_eq!(
                super::section_repos(open, &favourites, only),
                Some(favourites.clone())
            );
        }
        assert_eq!(super::section_repos(open, &[], false), Some(Vec::new()));
        for id in ["mine", "review", "involved"] {
            let section = super::section_by_id(id).unwrap();
            assert_eq!(
                super::section_repos(section, &favourites, false),
                None,
                "{id}"
            );
            assert_eq!(
                super::section_repos(section, &favourites, true),
                Some(favourites.clone()),
                "{id} with Favourites only"
            );
        }
    }

    #[test]
    fn empty_states_ask_for_favourites_where_they_are_missing() {
        let none = super::Scope {
            has_favourites: false,
            favourites_only: false,
        };
        let filtered = super::Scope {
            favourites_only: true,
            ..none
        };
        assert_eq!(empty_message("open", none).0, "No favourite repositories");
        assert_eq!(empty_message("open", ANY).0, "No open pull requests");
        assert!(super::needs_favourites("open", none));
        assert!(!super::needs_favourites("mine", none));
        assert_eq!(empty_message("mine", none), empty_message("mine", ANY));
        assert!(super::needs_favourites("mine", filtered));
        assert_eq!(
            empty_message("review", filtered).0,
            "No favourite repositories"
        );
        assert!(!super::needs_favourites("open", ANY));
    }

    #[test]
    fn empty_messages_cover_known_sections_and_unknown_fallback() {
        let empty_message = |section| empty_message(section, ANY);
        assert_eq!(
            empty_message("mine"),
            ("No open pull requests", "Pull requests you open show here.")
        );
        assert_eq!(
            empty_message("review"),
            (
                "Nothing waiting for your review",
                "Review requests for you and your teams show here."
            )
        );
        assert_eq!(
            empty_message("involved"),
            (
                "Nothing else involves you",
                "Open pull requests you comment on, review or are mentioned in show here."
            )
        );
        assert_eq!(
            empty_message("assigned"),
            (
                "No issues assigned to you",
                "Open issues assigned to this account show here."
            )
        );
        assert_eq!(
            empty_message("created"),
            (
                "No open issues you created",
                "Open issues you open show here."
            )
        );
        assert_eq!(
            empty_message("mentioned"),
            (
                "No open issues mention you",
                "Open issues that mention this account show here."
            )
        );
        assert_eq!(empty_message(""), empty_message("mentioned"));
    }

    #[test]
    fn groups_follow_the_newest_item() {
        let items = sample::items();
        let prs: Vec<_> = items
            .iter()
            .filter(|i| i.kind == subspace_framework::client_github::model::ItemKind::Pr)
            .cloned()
            .collect();
        let groups = group_by_repo(&prs);
        let names: Vec<&str> = groups.iter().map(|g| g.repo).collect();
        assert_eq!(names, ["octo-org/api", "octo-org/web", "octocat/subspace"]);
        let numbers: Vec<u32> = groups[0].items.iter().map(|i| i.number).collect();
        assert_eq!(numbers, [421, 415]);
    }

    #[test]
    fn check_summaries_count_failures_and_pending() {
        let check = |state| GhCheck {
            name: "x".into(),
            state,
            url: None,
        };
        assert_eq!(
            check_summary(&[check(CheckRunState::Success), check(CheckRunState::Success)]),
            "2 checks · all passed"
        );
        assert_eq!(
            check_summary(&[
                check(CheckRunState::Failure),
                check(CheckRunState::Cancelled),
                check(CheckRunState::Pending),
            ]),
            "3 checks · 2 failing · 1 pending"
        );
        assert_eq!(check_summary(&[]), "0 checks");
    }

    #[test]
    fn the_cache_drops_malformed_sections_and_items() {
        let item = serde_json::to_value(&sample::items()[0]).unwrap();
        let json = serde_json::json!({
            "mine": {"total": 3, "at": 5, "items": [item, {"bad": true}]},
            "review": {"total": "x", "at": 5, "items": []},
            "unknown": {"total": 1, "at": 5, "items": []},
        });
        let cache = parse_account_cache(&json);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache["mine"].items.len(), 1);
        assert_eq!(cache["mine"].total, 3);
    }

    #[test]
    fn a_fresh_item_replaces_it_in_every_section_and_search_sees_it_once() {
        let items = sample::items();
        let mut sections: HashMap<&'static str, SectionData> = HashMap::new();
        for id in ["mine", "involved"] {
            sections.insert(
                id,
                SectionData {
                    total: 1,
                    items: vec![items[0].clone()],
                    at: 0,
                },
            );
        }
        let mut fresh = items[0].clone();
        fresh.title = "Renamed".into();
        patch_sections(&mut sections, &fresh);
        assert!(sections.values().all(|s| s.items[0].title == "Renamed"));
        let found = search_items(&sections);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].0, "mine",
            "the first section in SECTIONS order owns it"
        );
    }

    #[test]
    fn context_text_reads_as_words_not_debug_output() {
        assert_eq!(kind_text(ItemKind::Pr), "Pull request");
        assert_eq!(kind_text(ItemKind::Issue), "Issue");
        assert_eq!(state_text(PrState::Open), "Open");
        assert_eq!(state_text(PrState::Draft), "Draft");
        assert_eq!(state_text(PrState::Merged), "Merged");
        assert_eq!(state_text(PrState::Closed), "Closed");
        assert_eq!(branch_text(Some(&"feat/x".to_owned())), "feat/x");
        assert_eq!(branch_text(Some(&String::new())), "None");
        assert_eq!(branch_text(None), "None");
    }

    #[test]
    fn keys_split_and_assignees_merge() {
        assert_eq!(split_key("octo/app#12"), Some(("octo/app", 12)));
        assert_eq!(split_key("octo/app"), None);
        let ann = GhUser {
            login: "ann".into(),
            name: Some("Ann".into()),
        };
        let me = GhUser {
            login: "ME".into(),
            name: Some("Me".into()),
        };
        let logins: Vec<String> =
            assignee_logins(std::slice::from_ref(&ann), "me", &[me, ann.clone()])
                .into_iter()
                .map(|u| u.login)
                .collect();
        assert_eq!(logins, ["ann", "me"]);
    }

    #[test]
    fn only_pull_request_section_ids_are_accepted_after_the_board_became_prs_only() {
        assert!(is_pr_section("mine"));
        assert!(is_pr_section("review"));
        assert!(is_pr_section("involved"));
        assert!(is_pr_section("open"));
        assert!(!is_pr_section("assigned"));
        assert!(!is_pr_section("created"));
        assert!(!is_pr_section("mentioned"));
        assert!(!is_pr_section("not-a-section"));
    }

    #[test]
    fn a_cache_saved_before_prs_only_drops_its_issue_sections_on_load() {
        let mut sections: HashMap<&'static str, SectionData> = HashMap::new();
        sections.insert(
            "mine",
            SectionData {
                total: 1,
                items: sample::items()
                    .into_iter()
                    .filter(|item| item.kind == ItemKind::Pr)
                    .collect(),
                at: 1,
            },
        );
        sections.insert(
            "assigned",
            SectionData {
                total: 2,
                items: sample::items()
                    .into_iter()
                    .filter(|item| item.kind == ItemKind::Issue)
                    .collect(),
                at: 1,
            },
        );
        let filtered = pr_only(sections);
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains_key("mine"));
        assert!(!filtered.contains_key("assigned"));
    }
}
