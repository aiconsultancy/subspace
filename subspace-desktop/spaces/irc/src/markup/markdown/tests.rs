//! Slack-style formatting rewritten as the shared rich text's Markdown: each
//! format, the word-boundary rule, precedence with code and links, Slack's
//! code fences and quotes, and what stays literal.

use std::time::{Duration, Instant};

use super::super::Mentions;
use super::emphasis::{Style, markers};
use super::spans::link;
use super::to_markdown;

fn mentions() -> Mentions {
    Mentions::new(["Mira", "theo", "nova-bot", "sam"])
}

/// The rewritten Markdown, or `None` when the text stays plain.
fn markdown(text: &str) -> Option<String> {
    to_markdown(text, &mentions()).map(|rendered| rendered.markdown)
}

/// The text as shown, for the row's accessible name.
fn plain(text: &str) -> Option<String> {
    to_markdown(text, &mentions()).map(|rendered| rendered.plain)
}

#[test]
fn each_slack_format_becomes_markdown() {
    for (text, expected, shown) in [
        ("*bold*", "**bold**", "bold"),
        ("**bold**", "**bold**", "bold"),
        ("_italic_", "_italic_", "italic"),
        ("~strike~", "~~strike~~", "strike"),
        ("~~strike~~", "~~strike~~", "strike"),
        ("run `cargo test`", "run `cargo test`", "run cargo test"),
        ("```\nlet x = 1;\n```", "```\nlet x = 1;\n```", "let x = 1;"),
        ("> quoted", "> quoted", "quoted"),
        ("- one\n- two", "- one\n- two", "- one\n- two"),
        ("• one\n• two", "- one\n- two", "• one\n• two"),
        ("1. one\n2. two", "1. one\n2. two", "1. one\n2. two"),
        (
            "[docs](https://example.org/docs)",
            "[docs](https://example.org/docs)",
            "docs",
        ),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
        assert_eq!(plain(text).as_deref(), Some(shown), "{text}");
    }
}

#[test]
fn plain_chat_stays_plain() {
    for text in [
        "morning all",
        "see https://example.org/a_b_c",
        "it's #4541, not a channel",
        "ask @theo in #ops-deck",
        "a lone ` backtick",
        "[not a link",
        ">.< oops",
        ">> not a quote",
    ] {
        assert_eq!(markdown(text), None, "{text}");
    }
}

#[test]
fn markers_inside_words_and_globs_stay_literal() {
    for text in [
        "snake_case_names",
        "_private_var",
        "__init__ runs first",
        "2*3*4 = 24",
        "ls *.rs and *.toml",
        "src/*.rs",
        "/usr/*/bin/*",
        "**/*.rs",
        "~/notes and ~/code",
        "a * b * c",
        "** spaced out **",
        "_ spaced _",
        "*bold*s",
        "match (*.rs) and (*.md)",
        "use \"*\" or \"*\"",
        "[*] and [*]",
        "***three***",
        "~~~tilde~~~",
        "*mismatched**",
    ] {
        assert_eq!(markdown(text), None, "{text}");
    }
    assert_eq!(
        markdown("*ok*: snake_case_name, 2*3*4, src/*.rs ~/x").as_deref(),
        Some("**ok**\\: snake\\_case\\_name, 2\\*3\\*4, src/\\*.rs \\~/x"),
        "literal markers are escaped once the message is Markdown"
    );
}

#[test]
fn markers_close_before_punctuation_and_open_after_brackets() {
    for (text, expected) in [
        ("*done*, thanks", "**done**, thanks"),
        ("(*aside*)", "(**aside**)"),
        ("\"*quoted*\"", "\"**quoted**\""),
        ("*Mira*'s turn", "**Mira**'s turn"),
        ("ship it *now*!", "ship it **now**!"),
        ("_why?_", "_why?_"),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn formats_nest_and_crossed_markers_stay_literal() {
    for (text, expected) in [
        ("*_both_*", "**_both_**"),
        ("_*both*_", "_**both**_"),
        ("~*gone*~", "~~**gone**~~"),
        ("**bold** and *also bold*", "**bold** and **also bold**"),
        ("*a _b* c_", "**a \\_b** c\\_"),
        ("*one* *two*", "**one** **two**"),
        ("*a*b*", "**a\\*b**"),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn the_pairing_rule_reports_each_marker() {
    let line = "*bold* _it_ ~gone~ x_y";
    let found: Vec<(&str, Style, bool)> = markers(line)
        .into_iter()
        .map(|marker| (&line[marker.range], marker.style, marker.opens))
        .collect();
    assert_eq!(
        found,
        [
            ("*", Style::Bold, true),
            ("*", Style::Bold, false),
            ("_", Style::Italic, true),
            ("_", Style::Italic, false),
            ("~", Style::Strike, true),
            ("~", Style::Strike, false),
        ]
    );
}

#[test]
fn code_is_never_formatted() {
    for (text, expected) in [
        ("`*not bold*` but *bold*", "`*not bold*` but **bold**"),
        ("*a `b*` c*", "**a `b*` c**"),
        ("``a`b`` and *x*", "``a`b`` and **x**"),
        (
            "```\n*x* _y_ # z @mira <b> [a](https://example.org)\n```",
            "```\n*x* _y_ # z @mira <b> [a](https://example.org)\n```",
        ),
        ("run `<script>` first", "run `<script>` first"),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
    assert_eq!(
        plain("run `<script>` first").as_deref(),
        Some("run <script> first"),
        "code that contains a tag keeps its text"
    );
    assert_eq!(
        plain("run ` a` and ` b `").as_deref(),
        Some("run  a and b"),
        "a code span loses one space at each end only when both ends have one"
    );
}

#[test]
fn long_words_are_rewritten_in_linear_time() {
    for word in ["a", "h", "x_"] {
        let text = format!("{} *done*", word.repeat(1 << 15));
        let rendered = to_markdown(&text, &mentions()).expect("formatted text");
        assert!(rendered.markdown.ends_with(" **done**"), "{word}");
    }
}

/// Rows are rewritten on every render, so a 64 KiB line of unclosed link
/// openings must stay cheap. Each `[` used to scan to the end of the line:
/// 28 s in a debug test build. Bounded at 32 nested parentheses, it takes
/// about 0.2 s there and far less in release; the bound leaves room for a
/// loaded machine while still catching quadratic work.
#[test]
fn pathological_links_are_rewritten_in_linear_time() {
    let text = format!("{} *done*", "[a](".repeat(1 << 14));
    let started = Instant::now();
    let rendered = to_markdown(&text, &mentions()).expect("formatted text");
    let elapsed = started.elapsed();
    assert!(rendered.markdown.ends_with(" **done**"));
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}

/// A destination nests at most 32 parentheses, as the parser allows; past
/// that it is not a link, and the scan stops there.
#[test]
fn a_destination_nests_at_most_32_parentheses() {
    let nested = |depth: usize| {
        format!(
            "[a](https://example.org/{}{})",
            "(".repeat(depth),
            ")".repeat(depth)
        )
    };
    assert!(link(&nested(32)).is_some(), "32 levels are a link");
    assert!(link(&nested(33)).is_none(), "33 levels are text");
}

#[test]
fn slack_code_fences_become_blocks() {
    for (text, expected) in [
        ("```let x = 1```", "```\nlet x = 1\n```"),
        (
            "```npm install\nnpm test```",
            "```\nnpm install\nnpm test\n```",
        ),
        ("```rust\nfn main() {}\n```", "```rust\nfn main() {}\n```"),
        ("```\ncode\n``` and after", "```\ncode\n```\nand after"),
        ("before\n```\ncode", "before\n```\ncode\n```"),
        ("    ```\n    indented\n    ```", "```\n    indented\n```"),
        ("``````\nsix\n``````", "```\nsix\n```"),
        ("```ls", "```\nls\n```"),
        ("```hello\n```", "```\nhello\n```"),
        (
            "``` ```\n[join](subspace-irc:channel/%23secret)\n```",
            "````\n```\n[join](subspace-irc:channel/%23secret)\n````",
        ),
        (
            "```\nsay ````hi```` now\n```",
            "`````\nsay ````hi```` now\n`````",
        ),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
    assert_eq!(
        plain("```npm install\nnpm test```").as_deref(),
        Some("npm install\nnpm test")
    );
}

#[test]
fn quotes_keep_their_lines() {
    for (text, expected) in [
        ("> one\n> two\nafter", "> one  \n> two\n\nafter"),
        ("before\n> quoted", "before\n\n> quoted"),
        (">\n> after a blank", ">  \n> after a blank"),
        (
            ">>> all\nof this\n\nquoted",
            "> all  \n> of this  \n>  \n> quoted",
        ),
        (">>>\nnext line", "> next line"),
        ("> *bold* in a quote", "> **bold** in a quote"),
        ("> - not a list", "> \\- not a list"),
        ("> 1. not a list", "> 1\\. not a list"),
        ("> # not a heading", "> \\# not a heading"),
        ("> > not nested", "> \\> not nested"),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn list_items_keep_their_text_and_stay_lists() {
    for (text, expected) in [
        ("- ---", "- \\-\\-\\-"),
        ("- > not a quote", "- \\> not a quote"),
        ("-     spaced", "- spaced"),
        ("    - indented\n    - list", "- indented\n- list"),
        ("- outer\n  - nested", "- outer\n  - nested"),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn links_open_only_for_safe_schemes() {
    for (text, expected) in [
        (
            "[wiki](https://example.org/wiki/Orbit_(ship)) *now*",
            "[wiki](https://example.org/wiki/Orbit_\\(ship\\)) **now**",
        ),
        (
            "[a](https://example.org/(b) *now*",
            "\\[a](https\\://example.org/(b) **now**",
        ),
        (
            "[mail](mailto:ops@example.org) *now*",
            "[mail](mailto:ops@example.org) **now**",
        ),
        (
            "[x](javascript:alert(1)) *now*",
            "\\[x](javascript\\:alert(1)) **now**",
        ),
        (
            "[x](file:///etc/passwd) *now*",
            "\\[x](file\\:///etc/passwd) **now**",
        ),
        (
            "[join](subspace-irc:channel/%23spam) *now*",
            "\\[join](subspace-irc\\:channel/%23spam) **now**",
        ),
        (
            "[*label*](https://example.org) *now*",
            "[\\*label\\*](https://example.org/) **now**",
        ),
        (
            "![tracker](https://example.org/p.png) *now*",
            "\\![tracker](https://example.org/p.png) **now**",
        ),
        (
            "*see https://example.org/a_b_c*",
            "**see [https\\:\\/\\/example\\.org\\/a\\_b\\_c](https://example.org/a_b_c)**",
        ),
        (
            "https://example.org/*glob* *now*",
            "[https\\:\\/\\/example\\.org\\/\\*glob\\*](https://example.org/*glob*) **now**",
        ),
        (
            "https://example.org/<b>x *now*",
            "https\\://example.org/&lt;b>x **now**",
        ),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn no_message_can_start_html() {
    for text in [
        "www.example.org<b>x *now*",
        "<script>alert(1)</script> *now*",
        "https://example.org/<img/src=x> *now*",
        "> <div>quoted</div>",
        "- <b>item</b>",
        "<!-- note --> _x_",
        "a\\<b> *now*",
        "[<b>x</b>](https://example.org)",
        "*bold*<i>x</i>",
        "see http://a<![CDATA[\nhidden ]]> *now*",
        "http://a<?pi x?> _x_",
    ] {
        let markdown = markdown(text).expect("formatted text");
        assert!(!markdown.contains('<'), "{text} became {markdown}");
    }
}

#[test]
fn html_entities_and_backslashes_stay_literal() {
    assert_eq!(
        markdown("<b>hi</b> &amp; $5 \\o/ C:\\temp\\*.log *now*").as_deref(),
        Some("&lt;b>hi&lt;/b> \\&amp; \\$5 \\\\o/ C\\:\\\\temp\\\\\\*.log **now**")
    );
}

#[test]
fn references_open_inside_formatting() {
    for (text, expected) in [
        (
            "*@mira please look*",
            "**[\\@mira](subspace-irc:nick/mira) please look**",
        ),
        (
            "_see #ops-deck_",
            "_see [\\#ops\\-deck](subspace-irc:channel/%23ops-deck)_",
        ),
        (
            "Mira: *ship it*",
            "[Mira](subspace-irc:nick/Mira)\\: **ship it**",
        ),
    ] {
        assert_eq!(markdown(text).as_deref(), Some(expected), "{text}");
    }
    assert_eq!(
        plain("*@mira please look* in #ops-deck").as_deref(),
        Some("@mira please look in #ops-deck")
    );
}

#[test]
fn bare_block_markers_and_underlines_stay_text() {
    assert_eq!(
        markdown("*note*\n-\n--\n=\n1.\n+").as_deref(),
        Some("**note**  \n\\-  \n\\-\\-  \n\\=  \n1\\.  \n\\+")
    );
}
