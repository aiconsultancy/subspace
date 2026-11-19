mod server;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use subspace_framework::client_github::Client;
use subspace_framework::core::config::{Config, web_boards::WebBoardSource};
use subspace_framework::core::credentials::{Credentials, Environment, GITHUB_ACCOUNT_KEY, Runner};
use subspace_framework::dashboard::{Services, prefs};

use crate::install::{ImportRequest, RepositoryRef};
use server::{BOARD, LOGIN, REPOSITORY, Server, TOKEN};

struct TestEnvironment(PathBuf);
impl Environment for TestEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| self.0.to_string_lossy().into_owned())
    }
}

struct TestRunner;
impl Runner for TestRunner {
    fn run(&self, program: &str, args: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        assert_eq!(program, "gh");
        assert_eq!(
            args,
            ["auth", "token", "--hostname", "github.com", "--user", LOGIN]
        );
        Ok(TOKEN.into())
    }
}

fn credentials(root: &std::path::Path) -> Arc<Credentials> {
    std::fs::write(
        root.join("hosts.yml"),
        format!("github.com:\n  user: {LOGIN}\n  users:\n    {LOGIN}: {{}}\n"),
    )
    .unwrap();
    Arc::new(Credentials::new(
        &Config::default(),
        TestEnvironment(root.into()),
        TestRunner,
    ))
}

fn weather_request(
    root: &std::path::Path,
    cx: &mut TestAppContext,
) -> (
    gpui_kit::AnyWindowHandle,
    gpui_kit::Entity<super::Manager>,
    super::Preparation,
) {
    let credentials = credentials(root);
    let (handle, manager) = super::super::tests::open_with_credentials(root, cx, Some(credentials));
    cx.update(|cx| {
        prefs::set(cx, GITHUB_ACCOUNT_KEY, LOGIN);
    });
    let preparation = cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("web-weather", cx);
        let preparation = super::request::from_editor(manager.read(cx), cx).unwrap();
        assert_eq!((&*preparation.id, &*preparation.title), ("weather", "Weather"));
        assert!(matches!(&preparation.request, ImportRequest::Repository { repository, reference: RepositoryRef::DefaultBranch, board_path } if repository == REPOSITORY && board_path == BOARD));
        preparation
    }).unwrap();
    (handle, manager, preparation)
}

fn built_weather() -> Vec<u8> {
    server::archive(&[
        (
            "repo/subspace-desktop/web-spaces/examples/weather/index.html",
            b"<p>test-only Weather</p>",
        ),
        (
            "repo/subspace-desktop/web-spaces/examples/weather/subspace-board.json",
            br#"{"name":"Weather","version":"test-only-version"}"#,
        ),
        ("repo/not-a-board.txt", b"excluded sibling"),
    ])
}

#[gpui_kit::test]
fn weather_form_validates_downloads_stages_and_confirms_only_selected_board(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(built_weather(), false);
    let (handle, manager, preparation) = weather_request(root.path(), cx);
    let (client, ticket) = cx.update(|cx| {
        (
            Client::new(cx.global::<Services>().credentials.clone()).with_api_url(&server.url),
            manager.update(cx, |manager, cx| manager.begin(cx)),
        )
    });
    let pending = super::prepare(
        preparation,
        &root.path().join("store"),
        client,
        Some(LOGIN.into()),
        ticket,
    )
    .unwrap_or_else(|error| panic!("Weather validation: {error}"));
    let WebBoardSource::Folder(folder) = pending.prepared.source() else {
        panic!("built folder")
    };
    assert_eq!(
        std::fs::read_to_string(folder.join("index.html")).unwrap(),
        "<p>test-only Weather</p>"
    );
    assert!(!folder.join("not-a-board.txt").exists());
    assert_eq!(
        std::fs::read_dir(folder.parent().unwrap()).unwrap().count(),
        1
    );
    assert_eq!(pending.prepared.version(), Some("test-only-version"));
    cx.update_window(handle, |_, window, cx| {
        assert!(cx.global::<Services>().config.web_boards.is_empty());
        manager.update(cx, |manager, cx| {
            manager.busy = false;
            manager.pending = Some(pending);
            cx.notify();
        });
        window.render_frame(cx);
        window.click("web-confirm-install", cx);
        assert_eq!(cx.global::<Services>().config.web_boards[0].id, "weather");
        assert_eq!(
            cx.global::<Services>().config.web_boards[0].title,
            "Weather"
        );
    })
    .unwrap();
    assert_saved_weather(root.path(), &folder);
}

fn assert_saved_weather(root: &std::path::Path, folder: &std::path::Path) {
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["futureSetting"], 42);
    let source = &saved["webBoards"]["weather"]["managedSource"];
    assert_eq!(source["repository"], REPOSITORY);
    assert_eq!(source["boardPath"], BOARD);
    assert_eq!(source["commit"], "b".repeat(40));
    assert_eq!(
        source["reference"],
        serde_json::json!({"kind":"branch","name":"main"})
    );
    assert!(
        folder.join("index.html").exists(),
        "confirmation retains the staged files"
    );
    assert!(!saved.to_string().contains(TOKEN));
}

#[gpui_kit::test]
fn remote_source_failure_reaches_manager_without_replacing_installed_files(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(Vec::new(), true);
    let (_, manager, preparation) = weather_request(root.path(), cx);
    let before = std::fs::read(root.path().join("config.json")).unwrap();
    let store = root.path().join("store");
    std::fs::create_dir_all(store.join("existing")).unwrap();
    std::fs::write(store.join("existing/index.html"), "previous installation").unwrap();
    let client = cx.update(|cx| {
        Client::new(cx.global::<Services>().credentials.clone()).with_api_url(&server.url)
    });
    let error = super::prepare(preparation, &store, client, Some(LOGIN.into()), 0)
        .err()
        .unwrap();
    assert!(
        error.contains("403"),
        "specific remote status must survive: {error}"
    );
    assert!(!error.contains("raw-test-only"));
    assert!(!error.contains(TOKEN));
    cx.update(|cx| {
        manager.update(cx, |manager, cx| {
            manager.fail(error.clone(), cx);
            assert_eq!(manager.error.as_deref(), Some(error.as_str()));
            assert!(manager.pending.is_none());
        })
    });
    assert_eq!(
        std::fs::read(root.path().join("config.json")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(store.join("existing/index.html")).unwrap(),
        "previous installation"
    );
    assert_eq!(std::fs::read_dir(store).unwrap().count(), 1);
}

#[gpui_kit::test]
fn repository_board_errors_identify_the_selected_directory_without_leaking_contents(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (_, manager, _) = weather_request(root.path(), cx);
    let before = std::fs::read(root.path().join("config.json")).unwrap();
    for (files, expected) in invalid_boards() {
        let server = Server::start(server::archive(&files), false);
        let (preparation, client) = cx.update(|cx| {
            (
                super::request::from_editor(manager.read(cx), cx).unwrap(),
                Client::new(cx.global::<Services>().credentials.clone()).with_api_url(&server.url),
            )
        });
        let error = super::prepare(
            preparation,
            &root.path().join("store"),
            client,
            Some(LOGIN.into()),
            0,
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            format!("GitHub board directory \"{BOARD}\": {expected}")
        );
        assert!(!error.contains("raw-test-only"));
        assert_eq!(
            std::fs::read(root.path().join("config.json")).unwrap(),
            before
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("store"))
                .unwrap()
                .count(),
            0
        );
    }
}

type InvalidBoard = (Vec<(&'static str, &'static [u8])>, &'static str);

fn invalid_boards() -> Vec<InvalidBoard> {
    vec![
        (
            vec![("repo/sibling.txt", b"not a board")],
            "The directory was not found in the repository archive.",
        ),
        (
            vec![(
                "repo/subspace-desktop/web-spaces/examples/weather/index.html",
                b"built app",
            )],
            "The board root needs a regular, readable subspace-board.json.",
        ),
        (
            vec![(
                "repo/subspace-desktop/web-spaces/examples/weather/subspace-board.json",
                br#"{"name":"Weather","version":"1"}"#,
            )],
            "The board root needs a regular, readable index.html. Select the built app folder.",
        ),
        (
            vec![
                (
                    "repo/subspace-desktop/web-spaces/examples/weather/index.html",
                    b"built app",
                ),
                (
                    "repo/subspace-desktop/web-spaces/examples/weather/subspace-board.json",
                    b"raw-test-only-malformed-manifest",
                ),
            ],
            "subspace-board.json: invalid JSON (expected value at line 1 column 1).",
        ),
    ]
}
