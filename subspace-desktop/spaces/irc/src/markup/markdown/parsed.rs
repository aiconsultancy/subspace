//! The rewritten Markdown parsed by the parser GPUI Kit's `TextView` uses:
//! the `markdown` crate with GFM and math, as `TextView` sets it up. No
//! message, however hostile, may yield HTML, an image, a heading, a rule, a
//! table, math or a definition, and every link must be one the link policy
//! opens or a reference the board found itself.

use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use subspace_framework::dashboard::links::safe_url;

use super::super::{Mentions, Ref, parse_ref_url};
use super::to_markdown;

/// Characters the generated messages are made of.
const ALPHABET: &[char] = &[
    '*', '_', '~', '`', '[', ']', '(', ')', '<', '>', '!', '\\', '&', '#', '$', '|', '-', ' ', 'a',
    '\n', '\r',
];
/// Messages around each generated piece: each formats something, so the
/// piece goes through the rewrite.
const TEMPLATES: &[&str] = &[
    "*x* {}",
    "[a](https://example.org/{}) {} _x_",
    "https://example.org/{} ~x~",
    "```{}\n{}\n```",
    "> {}\n- {}",
    "- *x*\n  - {}",
];

fn mentions() -> Mentions {
    Mentions::new(["Mira", "theo", "nova-bot", "sam"])
}

/// `TextView`'s parse options (`MarkdownExtensions::parse_options` in
/// gpui-base): GFM with inline and block math.
fn options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

/// Every node of `root`, depth first.
fn nodes(root: &Node) -> Vec<&Node> {
    let mut stack = vec![root];
    let mut out = Vec::new();
    while let Some(node) = stack.pop() {
        out.push(node);
        stack.extend(node.children().into_iter().flatten());
    }
    out
}

/// Whether a link may be shown: one the link policy opens, or a reference
/// whose text names what it opens.
fn allowed(url: &str, text: &str) -> bool {
    match parse_ref_url(url) {
        Some(Ref::Channel(name)) => text == name,
        Some(Ref::Nick(nick)) => text.trim_start_matches('@') == nick,
        Some(Ref::Issue(identifier) | Ref::Decision(identifier)) => text.contains(&identifier),
        Some(Ref::PullRequest(found)) => text.to_ascii_lowercase().contains(found.label()),
        None => safe_url(url).is_some(),
    }
}

/// What is wrong with the parsed rewrite of `text`, if anything.
fn problems(text: &str) -> Vec<String> {
    let Some(rendered) = to_markdown(text, &mentions()) else {
        return Vec::new();
    };
    let tree = to_mdast(&rendered.markdown, &options()).expect("the rewrite parses");
    nodes(&tree)
        .into_iter()
        .filter_map(|node| match node {
            Node::Link(link) => (!allowed(&link.url, &node.to_string()))
                .then(|| format!("link {:?} to {}", node.to_string(), link.url)),
            Node::Html(_)
            | Node::Image(_)
            | Node::ImageReference(_)
            | Node::LinkReference(_)
            | Node::Definition(_)
            | Node::FootnoteDefinition(_)
            | Node::FootnoteReference(_)
            | Node::Heading(_)
            | Node::ThematicBreak(_)
            | Node::Table(_)
            | Node::Math(_)
            | Node::InlineMath(_) => Some(format!("{node:?}")),
            _ => None,
        })
        .map(|problem| format!("{text:?} → {:?}: {problem}", rendered.markdown))
        .collect()
}

#[test]
fn hostile_messages_parse_without_html_images_or_forged_links() {
    let deep = format!(
        "[x](https://example.org/{}{}) *now*",
        "(".repeat(40),
        ")".repeat(40)
    );
    let found: Vec<String> = [
        "[x](https://example.org/<b>\\) shown *now*",
        "[x](https://example.org/<!--\\) hidden --> end *now*",
        "[x](https://example.org/[join](subspace-irc:nick/NickServ)\\) *now*",
        "https://example.org/\u{7f}[join](subspace-irc:channel/%23secret)x *now*",
        "https://example.org/\u{5}[join](subspace-irc:channel/%23secret) *now*",
        "``` ```\n[join](subspace-irc:channel/%23secret)\n```",
        "~ok~ https://example.org/><u>label</u>",
        "www.example.org<b>x *now*",
        "https://example.org/`<b>x</b>` *now*",
        "xhttps://example.org/`<b>x</b>` *now*",
        "see www.example.org`<b>x</b>` *now*",
        "(WWW.example.org`<b>x</b>`) *now*",
        "ops@example.org`<b>x</b>` *now*",
        "mailto:ops@example.org`<i>x</i>` *now*",
        "xmpp:ops@example.org`<i>x</i>` *now*",
        "*see www.example.org`[join](subspace-irc:channel/%23secret)`*",
        "www.example.org\\![t](https://example.org/x.png) *now*",
        "![t](https://example.org/x.png) *now*",
        "[join](subspace-irc:channel/%23spam) *now*",
        "[#ops-deck](subspace-irc:channel/%23secret) *now*",
        "*x*\r===",
        "*x*\r# heading",
        "*x*\n---",
        "*x*\n| a | b |\n|---|---|",
        "*x* $a$ and $$b$$",
        "*x*\n[a]: https://example.org",
        "*x* [^1]\n[^1]: note",
        "*x* &amp;lt;b&amp;gt; &#60;b&#62;",
        deep.as_str(),
    ]
    .into_iter()
    .flat_map(problems)
    .collect();
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn generated_messages_parse_without_html_images_or_forged_links() {
    let pieces: Vec<String> = ALPHABET
        .iter()
        .flat_map(|a| ALPHABET.iter().map(move |b| format!("{a}{b}")))
        .flat_map(|ab| ALPHABET.iter().map(move |c| format!("{ab}{c}")))
        .collect();
    let found: Vec<String> = TEMPLATES
        .iter()
        .flat_map(|template| {
            pieces
                .iter()
                .map(move |piece| template.replace("{}", piece))
        })
        .flat_map(|text| problems(&text))
        .take(5)
        .collect();
    assert!(found.is_empty(), "{found:#?}");
}

/// An empty nested list item must not underline the item above it as a
/// setext heading.
#[test]
fn empty_list_items_never_make_a_heading() {
    let found: Vec<String> = [
        "- shout\n  - ",
        "- a\n  - ",
        "- a\n   - ",
        "- a\n     - ",
        "- a\n\t- ",
        "- a\n  - \u{a0}",
        "- a\n  -",
        "• a\n  • ",
        "1. a\n   • ",
        "1. a\n   - ",
        "- [docs](https://example.org)\n  - ",
        "- `code` and #ops-deck\n  - ",
        "- @mira *now*\n  + ",
    ]
    .into_iter()
    .flat_map(problems)
    .collect();
    assert!(found.is_empty(), "{found:#?}");
    let rendered = to_markdown("- shout\n  - ", &mentions()).expect("a list");
    assert_eq!(
        rendered.markdown, "- shout\n\n\\- ",
        "the empty item is shown as typed text"
    );
}

/// The URLs the links in the rewrite of `text` open, as the parser reads
/// them.
fn opened(text: &str) -> Vec<String> {
    let rendered = to_markdown(text, &mentions()).expect("formatted text");
    let tree = to_mdast(&rendered.markdown, &options()).expect("the rewrite parses");
    nodes(&tree)
        .into_iter()
        .filter_map(|node| match node {
            Node::Link(link) => Some(link.url.clone()),
            _ => None,
        })
        .collect()
}

/// The URL a link opens is exactly the one the link policy approved, with
/// its brackets, backslashes and `&` as they are, for written links and
/// bare URLs alike. (A bare URL leaves out trailing punctuation such as `)`
/// or `;`, so the bare cases keep theirs inside.)
#[test]
fn links_open_exactly_the_approved_url() {
    for raw in [
        "https://example.org/wiki/Orbit_(ship)",
        "https://example.org/a(b(c)d)e?q=(x)#f(g)",
        "https://example.org/a\\b?x=y\\z#f\\g",
        "https://example.org/?a=1&amp;b=2&copy;",
        "mailto:ops@example.org",
    ] {
        let approved = safe_url(raw).expect("a safe URL");
        assert_eq!(opened(&format!("[label]({raw}) *x*")), [approved], "{raw}");
    }
    for raw in [
        "https://example.org/a(b(c)d)e?q=(x)y",
        "https://example.org/a\\b?x=y\\z#f\\g",
        "https://example.org/?a=1&amp;b=2&copy;c",
    ] {
        let approved = safe_url(raw).expect("a safe URL");
        assert_eq!(opened(&format!("see {raw} *x*")), [approved], "{raw}");
    }
}

#[test]
fn unicode_indentation_and_long_numbers_stay_text() {
    for text in [
        " - a\n\u{a0}- b",
        "  - a\n\u{3000}- b *x*",
        "\u{a0}\u{a0}- a\n - b",
        "1234567890. a\n1234567891. b *x*",
    ] {
        assert_eq!(problems(text), Vec::<String>::new(), "{text:?}");
    }
    let rendered =
        to_markdown("1234567890. a\n1234567891. b *x*", &mentions()).expect("formatted text");
    assert_eq!(
        rendered.markdown, "1234567890\\. a  \n1234567891\\. b **x**",
        "each line keeps its own line"
    );
}
