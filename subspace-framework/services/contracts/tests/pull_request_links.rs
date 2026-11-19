use subspace_contracts::github::{Github, OpenPullRequest, recognition::PullRequestLink};
use subspace_contracts::{Contract, MessageKey, MessageKind};

#[test]
fn navigation_role_is_named_and_versioned() {
    let key = MessageKey::request::<OpenPullRequest>();
    assert_eq!(Github::NAME, "github");
    assert_eq!(Github::VERSION, 1);
    assert_eq!(key.label(), "github@1/open-pull-request");
    assert_eq!(key.kind, MessageKind::Request);
    assert!(key.is_valid());
}

#[test]
fn canonical_links_keep_the_repository_identity_and_positive_number() {
    let link =
        PullRequestLink::parse("https://github.com/Octo-Org/repo_name.rs/pull/0042").unwrap();
    assert_eq!(link.repo(), "Octo-Org/repo_name.rs");
    assert_eq!(link.number(), 42);
    assert_eq!(link.key(), "Octo-Org/repo_name.rs#42");
    assert_eq!(
        link.url(),
        "https://github.com/Octo-Org/repo_name.rs/pull/42"
    );
    let longest = format!(
        "https://github.com/{}/{}/pull/999999999",
        "a".repeat(39),
        "r".repeat(100)
    );
    assert!(PullRequestLink::parse(&longest).is_some());
    assert!(PullRequestLink::parse("https://github.com/o/.github/pull/1").is_some());
}

#[test]
fn unrelated_or_ambiguous_urls_are_refused_without_normalisation() {
    for url in [
        "http://github.com/o/r/pull/1",
        "https://www.github.com/o/r/pull/1",
        "https://github.com.evil.test/o/r/pull/1",
        "https://github.example.test/o/r/pull/1",
        "https://user@github.com/o/r/pull/1",
        "https://user:pass@github.com/o/r/pull/1",
        "https://@github.com/o/r/pull/1",
        "https://github.com:443/o/r/pull/1",
        "https://github.com:8443/o/r/pull/1",
        "//github.com/o/r/pull/1",
        "https://github.com/o/r/issues/1",
        "https://github.com/o/r/pulls/1",
        "https://github.com/o/r/pull/1/files",
        "https://github.com/o/r/pull/1/",
        "https://github.com/o/r/pull/1?account=other",
        "https://github.com/o/r/pull/1?",
        "https://github.com/o/r/pull/1#discussion",
        "https://github.com/o/r/pull/1#",
        "https://github.com/o/r/pull/0",
        "https://github.com/o/r/pull/-1",
        "https://github.com/o/r/pull/+1",
        "https://github.com/o/r/pull/1.0",
        "https://github.com/o/r/pull/1000000000",
        "https://github.com/o/r/pull/4294967296",
        "https://github.com/o/r/pull/",
        "https://github.com/o/r/pull/١",
        " https://github.com/o/r/pull/1",
        "https://github.com/o/r/pull/1\n",
        "https://github.com/o/r/pull/%31",
        "https://github.com/o/r/pull/1\t",
        "https://github.com/o/%72/pull/1",
        "https://github.com/o/r/../r/pull/1",
        "https://github.com/o/r/%2e%2e/r/pull/1",
        "https://github.com/o/r\\pull/1",
    ] {
        assert!(PullRequestLink::parse(url).is_none(), "{url:?}");
    }
}

#[test]
fn owner_and_repository_names_are_checked() {
    for (owner, repo) in [
        ("", "r"),
        ("-owner", "r"),
        ("owner-", "r"),
        ("two--hyphens", "r"),
        ("owner.name", "r"),
        ("owner_name", "r"),
        ("ow ner", "r"),
        ("octö", "r"),
        ("o", ""),
        ("o", "."),
        ("o", ".."),
        ("o", "r%2fr"),
        ("o", "r name"),
        ("o", "répo"),
        ("o", "r@name"),
    ] {
        let url = format!("https://github.com/{owner}/{repo}/pull/1");
        assert!(PullRequestLink::parse(&url).is_none(), "{url}");
    }
    let owner = format!("https://github.com/{}/r/pull/1", "o".repeat(40));
    let repo = format!("https://github.com/o/{}/pull/1", "r".repeat(101));
    assert!(PullRequestLink::parse(&owner).is_none());
    assert!(PullRequestLink::parse(&repo).is_none());
}
