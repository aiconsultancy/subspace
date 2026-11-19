//! Plain and mIRC-coded text as literal Markdown, checked against the parser
//! GPUI Kit's `TextView` uses.

use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use subspace_framework::dashboard::Tone;

use super::literal;
use crate::markup::link::ref_url;
use crate::markup::{Mentions, Ref};
use crate::style::{Styled, styled, with_refs};

/// The inline tags the literal Markdown may hold, as the parser reads them.
const TAGS: &[&str] = &[
    "<b>", "</b>", "<i>", "</i>", "<u>", "</u>", "<s>", "</s>", "</mark>",
];

/// `TextView`'s parse options: GFM with inline and block math.
fn options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

/// Whether an HTML node is one of the inline tags, or a tint.
fn allowed_tag(html: &str) -> bool {
    TAGS.contains(&html) || (html.starts_with("<mark color=\"#") && html.ends_with("\">"))
}

/// The text `node` shows, with a line break for each hard break, and the
/// kinds of node it holds beyond paragraphs, text, code, links and our
/// inline tags.
fn shown(node: &Node, text: &mut String, other: &mut Vec<String>) {
    match node {
        Node::Text(found) => text.push_str(&found.value),
        Node::InlineCode(found) => text.push_str(&found.value),
        Node::Break(_) => text.push('\n'),
        Node::Html(found) if allowed_tag(&found.value) => {}
        Node::Root(_) | Node::Paragraph(_) | Node::Link(_) => {}
        other_node => other.push(format!("{other_node:?}")),
    }
    for child in node.children().into_iter().flatten() {
        shown(child, text, other);
    }
}

/// What `markdown` shows, and any node it should not hold.
fn parsed(markdown: &str) -> (String, Vec<String>) {
    let root = to_mdast(markdown, &options()).unwrap_or_else(|error| panic!("{error}"));
    let (mut text, mut other) = (String::new(), Vec::new());
    shown(&root, &mut text, &mut other);
    (text, other)
}

/// `styled` as literal Markdown without tints.
fn untinted(styled: &Styled) -> String {
    literal(styled, &|_| None)
}

/// Text that would be Markdown syntax, sent plain: each shows as typed.
const PLAIN: &[&str] = &[
    "*not bold*",
    "`x`",
    "[a](b)",
    "# not a heading",
    "1. not a list",
    "- not an item",
    "> not a quote",
    "<tag> and </b>",
    "a | b | c",
    "---",
    "$x$ and $$y$$",
    "&amp; &lt;",
    "~~not struck~~",
    "![alt](https://example.org/a.png)",
    "back\\slash \\* and _under_score_",
    "    four spaces of indentation",
    "\tand a tab",
    "line one\n  line two\n# line three",
    "ops@example.org www.example.org",
    "trailing spaces  ",
];

#[test]
fn plain_text_shows_exactly_as_sent() {
    for text in PLAIN {
        let markdown = untinted(&styled(text));
        assert_eq!(
            parsed(&markdown),
            ((*text).to_owned(), Vec::new()),
            "{text:?} as {markdown:?}"
        );
    }
}

/// Line endings and piece edges, with what each shows: every line ending,
/// a lone carriage return included, is one line break.
const EDGES: &[(&str, &str)] = &[
    ("a\rb", "a\nb"),
    ("a\r\nb", "a\nb"),
    ("a\r\r    code", "a\n\n    code"),
    ("a  \rb", "a  \nb"),
    ("a\n\nb", "a\n\nb"),
    ("\nlead", "\nlead"),
    ("a\n   \nb", "a\n   \nb"),
    ("a  \n", "a  "),
    ("é\u{3}4ü\u{3}  ", "éü  "),
    ("  ü", "  ü"),
    ("x\n\u{11}a``b\u{11}", "x\na``b"),
    ("end\\\nnext", "end\\\nnext"),
    ("end\\\u{2}bold\u{2}", "end\\bold"),
];

#[test]
fn line_endings_and_piece_edges_show_as_sent() {
    for (text, shown) in EDGES {
        let markdown = untinted(&styled(text));
        assert_eq!(
            parsed(&markdown),
            ((*shown).to_owned(), Vec::new()),
            "{text:?} as {markdown:?}"
        );
    }
}

#[test]
fn a_bare_url_is_a_link_that_shows_as_typed() {
    let markdown = untinted(&styled("see https://example.org/a_b."));
    assert_eq!(
        markdown,
        r"see [https\:\/\/example\.org\/a\_b](https://example.org/a_b)\."
    );
    assert_eq!(parsed(&markdown).0, "see https://example.org/a_b.");
}

#[test]
fn mirc_formatting_becomes_inline_tags_and_code() {
    let text =
        "\u{2}bold\u{2} \u{1d}italic\u{1d} \u{1f}under\u{1f} \u{1e}gone\u{1e} \u{11}a`b\u{11}";
    let markdown = untinted(&styled(text));
    assert_eq!(
        markdown,
        "<b>bold</b> <i>italic</i> <u>under</u> <s>gone</s> ``a`b``"
    );
    assert_eq!(
        parsed(&markdown),
        ("bold italic under gone a`b".to_owned(), Vec::new())
    );
}

#[test]
fn mirc_colours_become_tints_and_markers_beside_them_stay_literal() {
    let tint = |tone: Tone| (tone == Tone::Red).then(|| "#ff000029".to_owned());
    let markdown = literal(&styled("\u{3}4red\u{3} and *star*"), &tint);
    assert_eq!(
        markdown,
        "<mark color=\"#ff000029\">red</mark> and \\*star\\*"
    );
    assert_eq!(parsed(&markdown), ("red and *star*".to_owned(), Vec::new()));
    assert_eq!(
        untinted(&styled("\u{3}4red\u{3}")),
        "red",
        "without a tint the colour is dropped"
    );
}

#[test]
fn code_that_starts_or_ends_with_a_backtick_or_space_is_padded() {
    assert_eq!(untinted(&styled("\u{11}`x\u{11}")), "`` `x ``");
    assert_eq!(untinted(&styled("\u{11} x \u{11}")), "`  x  `");
    assert_eq!(parsed(&untinted(&styled("\u{11} x \u{11}"))).0, " x ");
}

#[test]
fn neighbouring_code_written_alike_is_one_span_so_backticks_never_touch() {
    let text = "\u{11}a\u{3}4b\u{3}c\u{11}";
    let markdown = untinted(&styled(text));
    assert_eq!(
        markdown, "`abc`",
        "the untinted colour does not split the code"
    );
    assert_eq!(parsed(&markdown), ("abc".to_owned(), Vec::new()));
    let tint = |tone: Tone| (tone == Tone::Red).then(|| "#ff000029".to_owned());
    let tinted = literal(&styled(text), &tint);
    assert_eq!(tinted, "`a`<mark color=\"#ff000029\">`b`</mark>`c`");
    assert_eq!(parsed(&tinted), ("abc".to_owned(), Vec::new()));
}

#[test]
fn references_become_board_links() {
    let members = Mentions::new(["alice"]);
    let markdown = untinted(&with_refs("ask @alice in #help", &members));
    let alice = ref_url(&Ref::Nick("alice".into()));
    let help = ref_url(&Ref::Channel("#help".into()));
    assert_eq!(
        markdown,
        format!("ask [\\@alice]({alice}) in [\\#help]({help})")
    );
    assert_eq!(parsed(&markdown).0, "ask @alice in #help");
}

#[test]
fn a_trailing_line_break_is_left_out() {
    assert_eq!(untinted(&styled("one\ntwo\n")), "one\\\ntwo");
}
