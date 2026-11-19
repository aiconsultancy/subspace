use subspace_client_irc::LineKind;
use subspace_framework::dashboard::Tone;

use super::{
    NICK_TONES, RunStyle, line_view, links, mirc_tone, nick_tone, styled, with_refs,
    with_unavailable,
};
use crate::fixtures::{channel, line, message};
use crate::markup::{IssueKeys, Mentions, Ref};
use crate::state::Entry;

fn entry(line: subspace_client_irc::Line) -> Entry {
    Entry {
        seq: 0,
        line,
        highlight: false,
    }
}

#[test]
fn nick_tones_are_stable_case_insensitive_and_varied() {
    assert_eq!(nick_tone("alice"), nick_tone("alice"));
    assert_eq!(nick_tone("Alice"), nick_tone("ALICE"));
    let nicks = [
        "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi",
    ];
    let mut tones: Vec<Tone> = nicks.iter().map(|nick| nick_tone(nick)).collect();
    tones.dedup();
    assert!(tones.len() > 2, "{tones:?}");
    assert!(tones.iter().all(|tone| NICK_TONES.contains(tone)));
}

#[test]
fn mirc_colours_map_to_theme_tones_only() {
    assert_eq!(mirc_tone(4), Some(Tone::Red));
    assert_eq!(mirc_tone(12), Some(Tone::Blue));
    assert_eq!(mirc_tone(1), None, "black follows the text colour");
    assert_eq!(mirc_tone(99), None);
}

#[test]
fn urls_become_links_without_trailing_punctuation() {
    let found = links("see https://example.org/a?b=1). and http://x.test");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].1, "https://example.org/a?b=1");
    assert_eq!(found[1].1, "http://x.test");
    assert!(links("https:// alone").is_empty());
}

#[test]
fn styled_text_marks_links_inside_plain_text() {
    let text = styled("docs at https://example.org now");
    assert_eq!(text.text, "docs at https://example.org now");
    let link = text.links[0].0.clone();
    assert_eq!(&text.text[link], "https://example.org");
    assert!(text.runs.is_empty(), "a link is not a formatting run");
}

#[test]
fn messages_lead_with_a_bold_nick_and_actions_are_italic() {
    let room = channel("#subspace");
    let said = line_view(&entry(message(&room, "alice", "hello", 0)));
    assert_eq!(said.styled.text, "alice hello");
    assert_eq!(
        said.styled.runs[0],
        (
            0..5,
            RunStyle {
                bold: true,
                ..RunStyle::default()
            }
        ),
        "a bold name without a tone, which a tint would mistake for mIRC colour"
    );
    assert!(!said.italic);
    let action = line_view(&entry(line(&room, LineKind::Action, "bob", "waves", 0)));
    assert_eq!(action.styled.text, "* bob waves");
    assert!(action.italic);
    let notice = line_view(&entry(line(&room, LineKind::Notice, "srv", "note", 0)));
    assert_eq!(
        (notice.styled.text.as_str(), notice.base),
        ("-srv- note", Tone::Muted)
    );
    let error = line_view(&entry(line(&room, LineKind::Error, "", "refused", 0)));
    assert_eq!(
        (error.styled.text.as_str(), error.base),
        ("refused", Tone::Red)
    );
}

#[test]
fn links_shift_with_the_nick_prefix() {
    let room = channel("#subspace");
    let view = line_view(&entry(message(&room, "alice", "https://example.org", 0)));
    let (range, url) = &view.styled.links[0];
    assert_eq!(&view.styled.text[range.clone()], url.as_str());
}

fn issue_mentions() -> Mentions {
    let scope = subspace_framework::contracts::issues::IssueScope::new(
        "dev",
        "Subspace API",
        "https://api.example.test/api",
    )
    .with_project_keys(["TST"]);
    Mentions::default().with_issues(IssueKeys::new(&scope))
}

#[test]
fn issue_urls_stay_ordinary_links() {
    let text = "see https://api.example.test/api/v1/issues/TST-8 and https://example.org";
    let styled = with_refs(text, &issue_mentions());
    assert_eq!(
        styled
            .links
            .iter()
            .map(|(_, url)| url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://api.example.test/api/v1/issues/TST-8",
            "https://example.org"
        ]
    );
    assert!(styled.refs.is_empty());
}

#[test]
fn unavailable_issue_references_are_marked_muted() {
    let styled = with_refs("TST-1 and TST-2", &issue_mentions());
    let styled = with_unavailable(styled, |identifier| identifier == "TST-2");
    let marked: Vec<(&str, bool, bool)> = styled
        .runs
        .iter()
        .map(|(range, style)| {
            (
                &styled.text[range.clone()],
                style.reference,
                style.unavailable,
            )
        })
        .collect();
    assert_eq!(marked, [("TST-1", true, false), ("TST-2", true, true)]);
}

#[test]
fn pull_request_urls_and_markers_are_references_rather_than_links() {
    let url = "https://github.com/example-org/subspace/pull/37/files";
    let text = format!("see {url}, [PR:octo/api#2] and https://example.org");
    let styled = with_refs(&text, &Mentions::default());
    assert_eq!(
        styled
            .links
            .iter()
            .map(|(_, url)| url.as_str())
            .collect::<Vec<_>>(),
        ["https://example.org"]
    );
    let refs: Vec<_> = styled
        .refs
        .iter()
        .map(|(range, found)| (&styled.text[range.clone()], found))
        .collect();
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert!(matches!(refs[0], (shown, Ref::PullRequest(found))
        if shown == url && found.key == "example-org/subspace#37" && found.url == url));
    assert!(matches!(refs[1], (shown, Ref::PullRequest(found))
        if shown == "[PR:octo/api#2]" && found.url == "https://github.com/octo/api/pull/2"));
}
