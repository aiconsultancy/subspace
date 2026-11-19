//! The Repositories control end to end against an offline GitHub: search,
//! star, the saved config, the running config, All open and Favourites only.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_framework::client_github::Client;
use subspace_framework::client_test_support::{Mock, Request};
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent, prefs};

use crate::{Account, Github, SectionRefresh, favourites};

fn pr(repo: &str, number: u32) -> Value {
    json!({
        "__typename": "PullRequest", "id": format!("{repo}#{number}"), "number": number,
        "title": format!("Change {number}"), "url": format!("https://github.com/{repo}/pull/{number}"),
        "state": "OPEN", "createdAt": "2026-09-01T00:00:00Z", "updatedAt": "2026-09-02T00:00:00Z",
        "repository": {"nameWithOwner": repo}, "author": {"login": "ann"},
        "assignees": {"nodes": []}, "labels": {"nodes": []}, "comments": {"totalCount": 0},
        "isDraft": false, "commits": {"nodes": []}
    })
}

/// Repository search answers, and one pull request per `repo:` qualifier.
fn answer(request: &Request) -> (u16, String) {
    if request.path.starts_with("/user/repos") {
        return (
            200,
            json!([{"full_name": "acme/web"}, {"full_name": "acme/api"}]).to_string(),
        );
    }
    if request.path.starts_with("/search/repositories") {
        return (
            200,
            json!({"items": [{"full_name": "acme/tools"}]}).to_string(),
        );
    }
    let body: Value = serde_json::from_str(&request.body).unwrap_or_default();
    let query = body["variables"]["query"].as_str().unwrap_or_default();
    let nodes: Vec<Value> = query
        .split(' ')
        .filter_map(|word| word.strip_prefix("repo:"))
        .map(|repo| pr(repo, 7))
        .collect();
    let count = nodes.len();
    (
        200,
        json!({"data": {"search": {"issueCount": count, "nodes": nodes}}}).to_string(),
    )
}

fn searches(mock: &Mock) -> Vec<String> {
    mock.requests()
        .iter()
        .filter(|request| request.method == "POST" && request.body.contains("isDraft"))
        .filter_map(|request| {
            let body: Value = serde_json::from_str(&request.body).ok()?;
            Some(body["variables"]["query"].as_str()?.to_owned())
        })
        .collect()
}

fn setup<'a>(
    root: &Path,
    repos: &[&str],
    mock: &Mock,
    cx: &'a mut TestAppContext,
) -> (Entity<Github>, &'a mut VisualTestContext) {
    let file = root.join("config.json");
    std::fs::write(&file, json!({"repos": repos}).to_string()).unwrap();
    let config = subspace_framework::core::config::Config {
        repos: repos.iter().map(|repo| (*repo).to_owned()).collect(),
        github: subspace_framework::core::config::GitHub {
            token: Some(subspace_framework::core::Secret::new(
                "test-only-nonworking",
            )),
        },
        ..Default::default()
    };
    let credentials = subspace_framework::agent_tools::test_support::credentials(&config);
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
        subspace_framework::dashboard::bind_keys(cx);
        Github::bind_keys(cx);
        cx.set_global(subspace_framework::dashboard::Services {
            credentials: credentials.clone(),
            config: Arc::new(config),
            tools: Default::default(),
            search: None,
            config_file: file,
        });
    });
    let url = mock.base_url().to_owned();
    let (board, cx) = cx.add_window_view(move |window, cx| {
        let mut board = Github::new(window, cx);
        board.client = Some(Arc::new(Client::new(credentials).with_api_url(url)));
        board.started = true;
        board.account = Some("ann".into());
        board.accounts = vec!["ann".into()];
        board.data.insert("ann".into(), Account::default());
        board.section = "open";
        board
    });
    settle(cx);
    (board, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn saved(root: &Path) -> Vec<String> {
    subspace_framework::core::settings::SettingsFile::open(&root.join("config.json"))
        .unwrap()
        .values()
        .repos
        .clone()
}

fn open_items(board: &Entity<Github>, cx: &mut VisualTestContext) -> Vec<String> {
    board.read_with(cx, |board, _| {
        board.data["ann"]
            .sections
            .get("open")
            .map(|data| data.items.iter().map(|item| item.key.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn search_star_saves_at_once_and_all_open_follows(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new(answer);
    let (board, cx) = setup(root.path(), &["acme/web"], &mock, cx);
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            seen.lock().unwrap().push(event.clone());
        })
    });
    board.update_in(cx, |board, window, cx| {
        board.load_all(SectionRefresh::Automatic, cx);
        board.open_repositories(None, window, cx);
    });
    settle(cx);
    assert_eq!(open_items(&board, cx), ["acme/web#7"]);
    cx.update(|window, _| assert!(window.try_find("menu").is_some()));
    cx.simulate_keystrokes("t o");
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(cx);
    // The filter row, then acme/tools from the search.
    cx.simulate_keystrokes("down enter");
    settle(cx);
    assert_eq!(saved(root.path()), ["acme/web", "acme/tools"]);
    cx.update(|_, cx| {
        assert_eq!(
            subspace_framework::dashboard::repositories::favourites(cx),
            ["acme/web", "acme/tools"]
        );
    });
    assert!(
        events
            .lock()
            .unwrap()
            .contains(&DashboardEvent::ConfigChanged)
    );
    assert_eq!(open_items(&board, cx), ["acme/web#7", "acme/tools#7"]);
    assert!(searches(&mock).iter().any(|query| {
        query.contains("repo:acme/web repo:acme/tools") && !query.contains("@me")
    }));
    cx.update(|window, _| assert!(window.try_find("menu").is_some(), "stays open"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(root.path().join("config.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[gpui_kit::test]
fn favourites_only_limits_the_personal_sections_and_is_remembered(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new(answer);
    let (board, cx) = setup(root.path(), &["acme/web"], &mock, cx);
    board.update(cx, |board, cx| board.toggle_favourites_only(cx));
    settle(cx);
    let personal: Vec<String> = searches(&mock)
        .into_iter()
        .filter(|query| query.contains("@me"))
        .collect();
    assert_eq!(personal.len(), 3, "{personal:?}");
    assert!(personal.iter().all(|query| query.contains("repo:acme/web")));
    cx.update(|_, cx| {
        assert_eq!(prefs::get(cx, favourites::ONLY_KEY).as_deref(), Some("1"));
    });
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.section_scope("mine"),
            Some(vec!["acme/web".to_owned()])
        );
    });
    board.update(cx, |board, cx| board.toggle_favourites_only(cx));
    board.read_with(cx, |board, _| assert_eq!(board.section_scope("mine"), None));
}

#[gpui_kit::test]
fn without_favourites_all_open_offers_to_add_repositories(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new(answer);
    let (board, cx) = setup(root.path(), &[], &mock, cx);
    board.update(cx, |board, cx| {
        board.load_all(SectionRefresh::Automatic, cx)
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("list-empty-configure").is_none());
        window.click("gh-empty-add-repositories", cx);
    });
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("menu").is_some()));
    assert!(searches(&mock).iter().all(|query| query.contains("@me")));
}

#[gpui_kit::test]
fn the_star_key_toggles_the_selected_repository_header(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new(answer);
    let (board, cx) = setup(root.path(), &["acme/web", "acme/api"], &mock, cx);
    board.update(cx, |board, cx| {
        board.load_all(SectionRefresh::Automatic, cx);
    });
    settle(cx);
    board.update_in(cx, |board, window, cx| {
        board.list.select("repo:acme/api".into());
        board.focus.focus(window, cx);
        assert_eq!(board.selected_repo().as_deref(), Some("acme/api"));
    });
    settle(cx);
    cx.simulate_keystrokes("f");
    settle(cx);
    assert_eq!(saved(root.path()), ["acme/web"]);
    board.read_with(cx, |board, _| {
        assert!(!board.favourites.contains("acme/api"));
        assert!(board.favourites.contains("acme/web"));
    });
}
