use super::link::ref_url;
use super::{Mentions, Ref, parse_ref_url, refs, rich, to_markdown};

fn mentions() -> Mentions {
    Mentions::new(["Mira", "theo", "nova-bot", "sam"])
}

/// The rewritten Markdown, or `None` when the text stays plain.
fn markdown(text: &str) -> Option<String> {
    to_markdown(text, &mentions()).map(|rendered| rendered.markdown)
}

#[test]
fn plain_chat_is_not_markdown() {
    for text in [
        "morning all",
        "the deploy window opens at 14:00",
        "2 * 3 = 6 and snake_case_name stays",
        "see https://example.org/a_b_c",
        "it's #4541, not a channel",
        "a lone ` backtick",
    ] {
        assert_eq!(rich(text, &mentions()), None, "{text}");
    }
}

#[test]
fn bot_reports_and_slack_formatting_are_markdown() {
    for text in [
        "**What happened:** done",
        "run `cargo test` first",
        "- one\n- two",
        "• built\n• tested",
        "1. review\n2. approve",
        "picking up the *on-call* rotation",
        "an _important_ note",
        "that plan is ~cancelled~",
        "> quoted",
        "```\ncode\n```",
        "[docs](https://example.org)",
    ] {
        assert!(rich(text, &mentions()).is_some(), "{text}");
    }
}

#[test]
fn mirc_codes_keep_the_irc_presentation() {
    for text in [
        "\u{2}loud\u{2} and *star*",
        "\u{3}04red\u{3} _note_",
        "*reset*\u{f}",
        "\u{1d}italic\u{1d} `code`",
    ] {
        assert_eq!(rich(text, &mentions()), None, "{text:?}");
    }
}

#[test]
fn channels_and_members_are_references() {
    let text = "Mira: approve it in #release-desk (ask @theo), not #4541 or @stranger";
    let found: Vec<(&str, Ref)> = refs(text, &mentions())
        .into_iter()
        .map(|(range, found)| (&text[range], found))
        .collect();
    assert_eq!(
        found,
        [
            ("Mira", Ref::Nick("Mira".into())),
            ("#release-desk", Ref::Channel("#release-desk".into())),
            ("@theo", Ref::Nick("theo".into())),
        ]
    );
    assert!(
        refs("ask theo: later", &mentions()).is_empty(),
        "only the first word addresses someone"
    );
}

#[test]
fn reference_links_round_trip_and_reject_anything_else() {
    for found in [
        Ref::Channel("#ops-deck".into()),
        Ref::Nick("we[ird]\\nick".into()),
    ] {
        assert_eq!(parse_ref_url(&ref_url(&found)), Some(found));
    }
    assert_eq!(parse_ref_url("https://example.org"), None);
    assert_eq!(parse_ref_url("subspace-irc:channel/"), None);
    assert_eq!(parse_ref_url("subspace-irc:channel/%zz"), None);
    assert_eq!(parse_ref_url("subspace-irc:other/x"), None);
}

#[test]
fn a_report_keeps_its_lines_lists_code_and_links() {
    let text = "**Nightly report** for #ops-deck\n\
                **What happened:** `orbit-api` deployed\n\
                • Build `a1b2c3d` passed\n\
                - Runbook: https://example.org/runbooks.\n\
                **Waiting on you:** @mira please approve\n\
                1. Review the diff";
    let rendered = to_markdown(text, &mentions()).expect("a report is Markdown");
    assert_eq!(
        rendered.markdown,
        "**Nightly report** for [\\#ops\\-deck](subspace-irc:channel/%23ops-deck)  \n\
         **What happened\\:** `orbit-api` deployed\n\
         \n\
         - Build `a1b2c3d` passed\n\
         - Runbook\\: [https\\:\\/\\/example\\.org\\/runbooks](https://example.org/runbooks).\n\
         \n\
         **Waiting on you\\:** [\\@mira](subspace-irc:nick/mira) please approve\n\
         \n\
         1. Review the diff"
    );
    assert_eq!(
        rendered.plain,
        "Nightly report for #ops-deck\n\
         What happened: orbit-api deployed\n\
         • Build a1b2c3d passed\n\
         - Runbook: https://example.org/runbooks.\n\
         Waiting on you: @mira please approve\n\
         1. Review the diff"
    );
}

#[test]
fn block_syntax_outside_the_subset_is_escaped() {
    let text = "*note*\n# Heading\n---\n<b>html</b> | table |\n    indented";
    assert_eq!(
        markdown(text).as_deref(),
        Some("**note**  \n\\# Heading  \n\\-\\-\\-  \n&lt;b>html&lt;/b> \\| table \\|  \nindented")
    );
}

#[test]
fn unbalanced_and_hostile_markup_stays_inert() {
    assert_eq!(markdown("a ` b *x*").as_deref(), Some("a \\` b **x**"));
    assert_eq!(
        markdown("[not a link *x*").as_deref(),
        Some("\\[not a link **x**")
    );
    assert_eq!(
        markdown("[join](subspace-irc:channel/%23spam) *now*").as_deref(),
        Some("\\[join](subspace-irc\\:channel/%23spam) **now**"),
        "a message cannot forge a reference link"
    );
    assert_eq!(
        markdown("```\nfn main() {}").as_deref(),
        Some("```\nfn main() {}\n```"),
        "an unclosed code block is closed"
    );
    assert_eq!(
        markdown("```\n# not a heading\n@mira\n```").as_deref(),
        Some("```\n# not a heading\n@mira\n```"),
        "code keeps its text"
    );
}

/// The list and fence messages that crashed the parser through the old IRC
/// path (`prepare_markdown` over the rewrite). The rewrite now writes fences
/// unindented at the left edge and hands `rich_text_prepared` its Markdown,
/// so the parser accepts them; the shared guard still stands behind it.
#[test]
fn list_fence_crashes_do_not_reach_the_parser_through_the_real_irc_path() {
    let mut crashes = Vec::new();
    for input in ["1. ```\n- x\n", "1. a\n   ```\n- x\n"] {
        let rendered = rich(input, &Mentions::default()).expect("a list is Markdown");
        eprintln!("IRC source={input:?}; rewritten={:?}", rendered.markdown);
        let parsed = std::panic::catch_unwind(|| {
            let mut options = markdown::ParseOptions::gfm();
            options.constructs.math_text = true;
            options.constructs.math_flow = true;
            markdown::to_mdast(&rendered.markdown, &options)
        });
        let parsed = parsed.expect("the vendored parser must contain unwinding panics");
        if let Err(error) = parsed {
            assert_eq!(error.reason, "The Markdown parser failed on this text.");
            crashes.push(rendered.markdown);
        }
    }
    assert_eq!(
        crashes,
        Vec::<String>::new(),
        "the IRC rewrite hands the parser no list and fence it rejects"
    );
}

#[test]
fn decision_references_round_trip_in_plain_and_formatted_messages() {
    let supported = Mentions::default().with_decisions(true);
    let message = "Read [d-00042-exa], D-100000-NAV and D-00001-X.";
    let expected = [
        Ref::Decision("d-00042-exa".into()),
        Ref::Decision("D-100000-NAV".into()),
    ];
    assert!(refs(message, &Mentions::default()).is_empty());
    assert_eq!(super::references(message, &supported, None), expected);

    let rendered = rich("*Read* D-00042-EXA, but ignore `D-100000-NAV`.", &supported).unwrap();
    assert_eq!(
        super::references("", &supported, Some(&rendered)),
        [Ref::Decision("D-00042-EXA".into())]
    );
    let decision = Ref::Decision("D-00042-EXA".into());
    assert_eq!(parse_ref_url(&ref_url(&decision)), Some(decision));
}

#[test]
fn an_irc_thread_link_is_clickable_as_a_bare_url_or_labelled_link() {
    let link = "subspace-irc:thread/%23example-ship/abc123";
    let expected = Ref::Thread("#example-ship".into(), "abc123".into());
    assert_eq!(parse_ref_url(link), Some(expected.clone()));
    for source in [format!("See {link}."), format!("[Original thread]({link})")] {
        let rendered = rich(&source, &Mentions::default()).unwrap();
        assert!(rendered.markdown.contains(link), "{}", rendered.markdown);
        assert_eq!(rendered.refs[0].1, expected);
    }
    for invalid in [
        "subspace-irc:thread/ada/abc123",
        "subspace-irc:thread/%23room/a%0Ab",
    ] {
        assert_eq!(parse_ref_url(invalid), None);
    }
}
