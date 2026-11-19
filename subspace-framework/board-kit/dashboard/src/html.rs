//! Service HTML to the Markdown that [`crate::rich`] shows.
//!
//! Markdown gives each paragraph its own source span, unlike the GPUI Kit HTML
//! parser. This module also applies the service HTML sanitising rules.

mod blocks;
mod entities;
mod parser;
mod tag;
mod writer;

pub(crate) use entities::decode_entities;
pub(crate) use parser::{Markup, attribute, markup, skip_element};
pub(crate) use writer::link_target;

/// Elements dropped with everything inside them.
pub(crate) const DROP: &[&str] = &[
    "script", "style", "template", "noscript", "iframe", "object", "embed", "svg", "math", "head",
    "title", "form", "button", "select", "textarea",
];

/// Converts sanitised `html` to Markdown.
pub fn to_markdown(html: &str) -> String {
    convert(html, true)
}

/// Text-only image placeholders, for views that render authenticated images separately.
// qual:api
pub fn to_markdown_without_images(html: &str) -> String {
    convert(html, false)
}

fn convert(html: &str, images: bool) -> String {
    let mut writer = writer::Writer::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        writer.text(&decode_entities(&rest[..start]));
        let text = &rest[start..];
        match markup(text) {
            None => {
                writer.text("<");
                rest = &text[1..];
            }
            Some(Markup::Skip(len)) => rest = &text[len.min(text.len())..],
            Some(Markup::Tag(tag)) => {
                rest = &text[tag.len..];
                if DROP.contains(&tag.name.as_str()) {
                    if !tag.closing {
                        rest = skip_element(rest, &tag.name);
                    }
                } else if !images && tag.name == "img" {
                    writer.text(
                        &attribute(tag.attributes, "alt").unwrap_or_else(|| "[Image]".into()),
                    );
                } else {
                    tag::write_tag(&mut writer, &tag);
                }
            }
        }
    }
    writer.text(&decode_entities(rest));
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::{decode_entities, to_markdown};

    #[test]
    fn paragraphs_with_inline_code_stay_separate_paragraphs() {
        let html = "<h3>Outcome </h3><p>The observer inside <code>Eva.Job.Handler</code> reconciles Jobs. </p><h3>Scope </h3><p><code>queued</code> first, then <code>{requestGuid}/lifecycle.json</code>. </p>";
        assert_eq!(
            to_markdown(html),
            "### Outcome\n\nThe observer inside `Eva.Job.Handler` reconciles Jobs.\n\n### Scope\n\n`queued` first, then `{requestGuid}/lifecycle.json`."
        );
    }

    #[test]
    fn lists_nest_and_items_keep_their_code() {
        let html = "<ul><li>Map to <code>queued</code> or <code>failed</code>. </li><li>Parent<ol><li>one</li><li>two</li></ol></li> </ul><p>After</p>";
        assert_eq!(
            to_markdown(html),
            "- Map to `queued` or `failed`.\n- Parent\n  1. one\n  2. two\n\nAfter"
        );
    }

    #[test]
    fn marks_links_and_unsafe_content() {
        let html = r#"<div>Hi <b> bold</b> and <i>it*</i> <a href="javascript:alert(1)">bad</a> <a href='https://dev.azure.com/a?b=1&amp;c=2'>good</a><script>alert(1)</script><!-- x --></div><div>1 &lt; 2 &amp;&nbsp;done<br>next</div>"#;
        assert_eq!(
            to_markdown(html),
            "Hi **bold** and *it\\** bad [good](https://dev.azure.com/a?b=1&c=2)\n\n1 \\< 2 & done\\\nnext"
        );
    }

    #[test]
    fn images_tables_and_pre() {
        let html = r#"<p><img src="https://x.example/a.png" alt="Shot"><img src="file:///x"></p><table><tr><th>A</th><th>B</th></tr><tr><td>1 | 2</td><td><b>3</b></td></tr></table><pre>let a = 1;
let b = a &lt; 2;</pre>"#;
        assert_eq!(
            to_markdown(html),
            "[\\[Shot ↗\\]](https://x.example/a.png)\n\n| A | B |\n| --- | --- |\n| 1 \\| 2 | **3** |\n\n```\nlet a = 1;\nlet b = a < 2;\n```"
        );
    }

    #[test]
    fn entities_decode() {
        assert_eq!(
            decode_entities("&#39;&#x41;&hellip;&bogus;&"),
            "'A…&bogus;&"
        );
    }

    #[test]
    fn malformed_and_nested_markup_is_safe() {
        assert_eq!(
            to_markdown("<div><b>ok</div><a href='data:text/plain,x'>no</a><"),
            "**ok**\n\nno\\<"
        );
        assert_eq!(to_markdown("<script><b>drop</b></script>keep"), "keep");
    }
}
