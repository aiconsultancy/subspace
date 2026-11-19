//! Diffs, file contents, open pull requests and the paging helpers.

use std::sync::Arc;

use serde_json::json;

use subspace_core::http::Error;

use super::{Handler, Request, client, serve, token};
use crate::file_pages;

#[test]
fn a_diff_uses_the_merge_base_and_pages_files() {
    let head = "b".repeat(40);
    let base = "a".repeat(40);
    let merge = "c".repeat(40);
    let (h, b, m) = (head.clone(), base.clone(), merge.clone());
    let handler: Arc<Handler> = Arc::new(move |request: &Request| {
        let line = &request.line;
        if line.starts_with("GET /repos/o/r/pulls/5 ") {
            let pull = json!({"head": {"sha": h}, "base": {"sha": b}, "changed_files": 101});
            return (200, pull.to_string());
        }
        if line.starts_with("GET /repos/o/r/compare/") {
            assert!(line.contains(&format!("{b}...{h}?per_page=1&page=2")));
            return (200, json!({"merge_base_commit": {"sha": m}}).to_string());
        }
        if line.starts_with("GET /repos/o/r/pulls/5/files?per_page=100&page=1 ") {
            let files: Vec<_> = (0..100)
                .map(|i| json!({"filename": format!("f{i}"), "status": "added"}))
                .collect();
            return (200, json!(files).to_string());
        }
        if line.starts_with("GET /repos/o/r/pulls/5/files?per_page=100&page=2 ") {
            return (
                200,
                json!([{"filename": "last", "status": "removed"}]).to_string(),
            );
        }
        (500, "{}".into())
    });
    let client = client(serve(handler), &token("tok"));
    let diff = client.diff("ann", "o/r#5").unwrap();
    assert_eq!(diff.base_sha, merge);
    assert_eq!(diff.head_sha, head);
    assert_eq!(diff.total, 101);
    assert_eq!(diff.files.len(), 101);
    assert_eq!(diff.files[100].path, "last");
}

#[test]
fn files_are_text_and_paths_are_encoded() {
    let handler: Arc<Handler> = Arc::new(|request: &Request| {
        if request.line.contains("/contents/dir/a%20b.rs?ref=") {
            return (200, "fn main() {}".into());
        }
        (200, "bin\u{0}ary".into())
    });
    let client = client(serve(handler), &token("tok"));
    let sha = "d".repeat(40);
    let file = client.file("ann", "o/r", &sha, "dir/a b.rs").unwrap();
    assert_eq!(file.text, "fn main() {}");
    let binary = client.file("ann", "o/r", &sha, "x.bin").unwrap_err();
    assert!(matches!(binary, Error::Status { status: 415, .. }));
    let not_image = client.image("ann", "o/r", &sha, "x.rs").unwrap_err();
    assert!(matches!(not_image, Error::Status { status: 415, .. }));
}

#[test]
fn file_page_counts() {
    assert_eq!(file_pages(0), 1);
    assert_eq!(file_pages(100), 1);
    assert_eq!(file_pages(101), 2);
    assert_eq!(file_pages(10_000), 30);
}
