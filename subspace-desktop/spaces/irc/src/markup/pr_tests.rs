//! GitHub pull request references in message text: the canonical
//! `[PR:owner/repo#n]` marker, full pull request URLs with GitHub's own
//! suffixes, and a Markdown link whose label is the marker. Each names one
//! canonical `owner/repo#n` identity and keeps the URL as written.

use super::link::ref_url;
use super::{Mentions, PrRef, Ref, parse_ref_url, pull_requests, refs, to_markdown};

fn found(text: &str) -> Vec<(String, Ref)> {
    refs(text, &Mentions::default())
        .into_iter()
        .map(|(range, found)| (text[range].to_owned(), found))
        .collect()
}

/// The references a rendered message lists, without where they are.
fn found_in(rendered: &super::Rendered) -> Vec<Ref> {
    rendered
        .refs
        .iter()
        .map(|(_, found)| found.clone())
        .collect()
}

fn pr(key: &str, url: &str) -> Ref {
    Ref::PullRequest(PrRef {
        key: key.into(),
        url: url.into(),
    })
}

const CANONICAL: &str = "https://github.com/example-org/subspace/pull/37";

#[test]
fn the_marker_names_its_pull_request_with_the_canonical_url() {
    assert_eq!(
        found("see ([PR:example-org/subspace#37]). thanks"),
        [(
            "[PR:example-org/subspace#37]".to_owned(),
            pr("example-org/subspace#37", CANONICAL)
        )]
    );
}

#[test]
fn pull_request_urls_with_github_suffixes_share_one_identity_and_keep_the_original() {
    let urls = [
        CANONICAL,
        "https://github.com/example-org/subspace/pull/37/",
        "https://github.com/example-org/subspace/pull/37/files",
        "https://github.com/example-org/subspace/pull/37/commits",
        "https://github.com/example-org/subspace/pull/37?w=1",
        "https://github.com/example-org/subspace/pull/37#discussion_r1234",
        "https://github.com/example-org/subspace/pull/37/files#diff-abc",
    ];
    for url in urls {
        let text = format!("review ({url}), please");
        assert_eq!(
            found(&text),
            [(url.to_owned(), pr("example-org/subspace#37", url))],
            "{url}"
        );
    }
    let all = urls.join(" ");
    assert_eq!(
        pull_requests(
            &format!("{all} [PR:example-org/subspace#37]"),
            &Mentions::default(),
            None
        ),
        ["example-org/subspace#37"],
        "one identity for the marker and every URL"
    );
}

#[test]
fn other_github_links_stay_links_and_unqualified_numbers_stay_text() {
    for text in [
        "https://github.com/example-org/subspace/issues/37",
        "http://github.com/example-org/subspace/pull/37",
        "https://github.com/example-org/subspace/pull/37/checks/extra",
        "https://github.com/example-org/subspace/pulls",
        "https://example.org/example-org/subspace/pull/37",
        "fixed in #123 already",
        "[PR:subspace#37] has no owner",
        "[PR:example-org/subspace#0] [PR:example-org/subspace#x] [PR:/subspace#3]",
        "PR:example-org/subspace#37 without brackets",
    ] {
        assert!(found(text).is_empty(), "{text}: {:?}", found(text));
        assert!(
            pull_requests(text, &Mentions::default(), None).is_empty(),
            "{text}"
        );
    }
}

#[test]
fn a_markdown_link_labelled_with_its_marker_is_one_reference() {
    let url = "https://github.com/example-org/subspace/pull/37/files";
    for text in [
        format!("[PR:example-org/subspace#37]({url})"),
        format!("[[PR:example-org/subspace#37]]({url})"),
    ] {
        assert_eq!(
            found(&text),
            [(text.clone(), pr("example-org/subspace#37", url))],
            "{text}"
        );
        let rendered = to_markdown(&format!("*see* {text}"), &Mentions::default()).unwrap();
        assert_eq!(found_in(&rendered), [pr("example-org/subspace#37", url)]);
        assert_eq!(
            rendered.markdown.matches("subspace-irc:pr/").count(),
            1,
            "{}",
            rendered.markdown
        );
        assert!(!rendered.markdown.contains(&format!("({url})")));
    }
}

#[test]
fn a_marker_linked_to_another_pull_request_stays_an_ordinary_link() {
    let text =
        "*see* [PR:example-org/subspace#37](https://github.com/example-org/subspace/pull/38)";
    let rendered = to_markdown(text, &Mentions::default()).unwrap();
    assert!(rendered.refs.is_empty(), "{}", rendered.markdown);
    assert!(!rendered.markdown.contains("subspace-irc:pr/"));
}

#[test]
fn formatted_messages_link_pull_requests_outside_code() {
    let text = "*ship* [PR:example-org/subspace#37] and `[PR:example-org/subspace#38]` \
                then https://github.com/octo/subspace/pull/2?x=1";
    let rendered = to_markdown(text, &Mentions::default()).unwrap();
    assert_eq!(
        found_in(&rendered),
        [
            pr(
                "example-org/subspace#37",
                "https://github.com/example-org/subspace/pull/37"
            ),
            pr(
                "octo/subspace#2",
                "https://github.com/octo/subspace/pull/2?x=1"
            ),
        ]
    );
    assert!(
        rendered.markdown.contains("`[PR:example-org/subspace#38]`"),
        "code stays plain: {}",
        rendered.markdown
    );
    assert_eq!(
        rendered
            .plain
            .matches("[PR:example-org/subspace#37]")
            .count(),
        1
    );
}

#[test]
fn pull_request_reference_links_round_trip_the_original_url() {
    for (key, url) in [
        ("example-org/subspace#37", CANONICAL),
        (
            "example-org/subspace#37",
            "https://github.com/example-org/subspace/pull/37/files?w=1#discussion_r9",
        ),
    ] {
        let found = pr(key, url);
        let link = ref_url(&found);
        assert!(link.starts_with("subspace-irc:pr/"), "{link}");
        assert!(
            !link.contains(['(', ')', ' ', '<', '>', '#', '?']),
            "{link}"
        );
        assert_eq!(parse_ref_url(&link), Some(found));
    }
    assert_eq!(
        parse_ref_url("subspace-irc:pr/https%3A%2F%2Fexample.org%2Fx"),
        None,
        "only a pull request URL is a pull request reference"
    );
}

#[test]
fn a_chip_names_the_owner_only_beside_a_same_named_repository_of_another_owner() {
    let api = PrRef::from_url("https://github.com/octo-org/api/pull/421").unwrap();
    let web = PrRef::from_url("https://github.com/octo-org/web/pull/7").unwrap();
    let other_api = PrRef::from_url("https://github.com/Acme/API/pull/5").unwrap();
    let same_owner = PrRef::from_url("https://github.com/octo-org/api/pull/9").unwrap();
    assert_eq!(
        api.label_among(&[api.clone(), web.clone(), same_owner]),
        "api#421"
    );
    assert_eq!(
        api.label_among(&[api.clone(), other_api.clone()]),
        "octo-org/api#421"
    );
    assert_eq!(
        other_api.label_among(&[api.clone(), other_api.clone()]),
        "acme/api#5"
    );
    assert_eq!(web.label_among(&[api, other_api, web.clone()]), "web#7");
}

#[test]
fn the_pull_requests_a_rewritten_message_links_are_found_in_order() {
    let api = PrRef::from_url("https://github.com/octo-org/api/pull/421/files").unwrap();
    let marker = PrRef::from_url("https://github.com/acme/api/pull/5").unwrap();
    let source = format!(
        "see [x]({}) and [y]({}) or [z](https://example.org)",
        ref_url(&Ref::PullRequest(api.clone())),
        ref_url(&Ref::PullRequest(marker.clone()))
    );
    assert_eq!(super::linked_pull_requests(&source), [api, marker]);
}

#[test]
fn rendered_references_keep_where_they_are_in_the_message() {
    let text = "*see* TST-1 and [PR:octo/api#4]\n\
                > quoted [PR:octo/api#5]\n\
                - item https://github.com/octo/api/pull/6\n\
                `[PR:octo/api#7]`";
    let mentions = Mentions::default().with_issues(super::IssueKeys::new(
        &subspace_framework::contracts::issues::IssueScope::new(
            "dev",
            "API",
            "https://api.example.test",
        )
        .with_project_keys(["TST"]),
    ));
    let rendered = to_markdown(text, &mentions).unwrap();
    let shown: Vec<&str> = rendered
        .refs
        .iter()
        .map(|(range, _)| &text[range.clone()])
        .collect();
    assert_eq!(
        shown,
        [
            "TST-1",
            "[PR:octo/api#4]",
            "[PR:octo/api#5]",
            "https://github.com/octo/api/pull/6"
        ]
    );
}
