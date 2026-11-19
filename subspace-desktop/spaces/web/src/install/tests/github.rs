use super::super::{ManagedSource, RepositoryRef, check_update, prepare};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use subspace_framework::client_github::{BoundRead, Client};
use subspace_framework::core::{
    config::Config,
    credentials::{Credentials, Environment, Runner},
};

struct TestEnvironment;
impl Environment for TestEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| "dummy-test-only-nonworking-token".into())
    }
}
struct NoProcesses;
impl Runner for NoProcesses {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        panic!("The package test must never execute a process");
    }
}

type Handler = dyn Fn(&str, &Value) -> (u16, Vec<u8>) + Send + Sync;

fn binding(handler: Arc<Handler>) -> BoundRead {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0;
            let mut authorized = false;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                if lower.starts_with("authorization:") {
                    authorized = header.contains("dummy-test-only-nonworking-token");
                }
            }
            assert!(authorized);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let (status, bytes) = if body["query"] == "query ViewerBound { viewer { id login } }" {
                (
                    200,
                    serde_json::to_vec(
                        &json!({"data":{"viewer":{"id":"U_test","login":"test-user"}}}),
                    )
                    .unwrap(),
                )
            } else {
                handler(&line, &body)
            };
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            stream.write_all(&bytes).unwrap();
        }
    });
    let credentials = Credentials::new(&Config::default(), TestEnvironment, NoProcesses);
    Client::new(Arc::new(credentials))
        .with_api_url(url)
        .bind_read("test-user")
        .unwrap()
}

fn authorized(line: &str) -> Option<Value> {
    if line.starts_with("GET /user/memberships/orgs/acme ") {
        Some(json!({"state":"active","organization":{"login":"acme"}}))
    } else if line.starts_with("GET /repos/acme/board ") {
        Some(
            json!({"full_name":"acme/board","default_branch":"trunk","owner":{"type":"Organization"},"permissions":{"pull":true}}),
        )
    } else {
        None
    }
}

fn repository(reference: RepositoryRef) -> ManagedSource {
    ManagedSource::Repository {
        repository: "acme/board".into(),
        reference,
        board_path: "apps/board".into(),
        commit: "a".repeat(40),
        content: "test-content".into(),
    }
}

#[test]
fn branch_checks_only_offer_and_prepare_pins_commit_without_installing_siblings() {
    let downloaded = Arc::new(AtomicUsize::new(0));
    let seen = downloaded.clone();
    let archive = super::archive(&[
        ("repo/apps/board/index.html", b"<p>test</p>"),
        (
            "repo/apps/board/subspace-board.json",
            br#"{"name":"Test","version":"2"}"#,
        ),
        ("repo/private-sibling.txt", b"not installed"),
    ]);
    let bound = binding(Arc::new(move |line, _| {
        if line.contains("/zipball/") {
            assert!(line.contains(&"b".repeat(40)));
            seen.fetch_add(1, Ordering::SeqCst);
            return (200, archive.clone());
        }
        let body = authorized(line).unwrap_or_else(|| {
            assert!(line.contains("/commits/"));
            json!({"sha":"b".repeat(40)})
        });
        (200, serde_json::to_vec(&body).unwrap())
    }));
    let offer = check_update(&repository(RepositoryRef::Branch("trunk".into())), &bound)
        .unwrap()
        .unwrap();
    assert_eq!(downloaded.load(Ordering::SeqCst), 0);
    assert_eq!(offer.installed(), "a".repeat(40));
    assert_eq!(offer.available(), "b".repeat(40));
    assert!(offer.label().starts_with("Update from "));
    let store = tempfile::tempdir().unwrap();
    let prepared = prepare(offer.request(), store.path(), Some(&bound)).unwrap();
    let subspace_framework::core::config::web_boards::WebBoardSource::Folder(root) =
        prepared.source()
    else {
        panic!("folder");
    };
    assert!(root.join("index.html").exists());
    assert!(!root.join("private-sibling.txt").exists());
    assert_eq!(
        std::fs::read_dir(root.parent().unwrap()).unwrap().count(),
        1
    );
    assert_eq!(
        prepared.metadata().revision(),
        Some("b".repeat(40).as_str())
    );
    assert!(
        matches!(prepared.metadata(), ManagedSource::Repository { reference: RepositoryRef::Branch(name), .. } if name == "trunk")
    );
}

#[test]
fn pinned_tags_and_commits_never_become_branch_updates() {
    let bound = binding(Arc::new(move |line, _| {
        let body = authorized(line).unwrap_or_else(|| {
            assert!(line.contains("/commits/"));
            json!({"sha":"b".repeat(40)})
        });
        (200, serde_json::to_vec(&body).unwrap())
    }));
    for reference in [
        RepositoryRef::Tag("v1".into()),
        RepositoryRef::Commit("a".repeat(40)),
    ] {
        assert!(
            check_update(&repository(reference), &bound)
                .unwrap()
                .is_none()
        );
    }
}
