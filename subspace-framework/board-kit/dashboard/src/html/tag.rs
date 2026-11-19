use super::parser::{Tag, attribute};
use super::writer::{Writer, escape_markdown, link_target};
use crate::links::safe_url;

pub(crate) fn write_tag(writer: &mut Writer, tag: &Tag<'_>) {
    if writer.pre.is_some() && !(tag.name == "pre" && tag.closing) {
        if tag.name == "br" {
            writer.text("\n");
        }
        return;
    }
    if writer.code.is_some() && !(tag.name == "code" && tag.closing) {
        return;
    }
    match (tag.name.as_str(), tag.closing) {
        ("p" | "div" | "section" | "article" | "header" | "footer" | "dl" | "dt" | "dd", _) => {
            writer.block()
        }
        ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => heading_start(writer, &tag.name),
        ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => {
            writer.heading = false;
            writer.block();
        }
        ("br", _) => writer.line_break(),
        ("hr", _) => {
            writer.block();
            writer.out.push_str("---");
            writer.fresh = false;
            writer.block();
        }
        ("ul", false) => writer.open_list(false),
        ("ol", false) => writer.open_list(true),
        ("ul" | "ol", true) => writer.close_list(),
        ("li", false) => writer.open_item(),
        ("li", true) => writer.close_marks(),
        ("blockquote", false) => quote_start(writer),
        ("blockquote", true) => quote_end(writer),
        ("pre", false) => writer.start_pre(),
        ("pre", true) => writer.finish_pre(),
        ("code" | "kbd" | "samp" | "tt", false) => writer.set_code(),
        ("code" | "kbd" | "samp" | "tt", true) => writer.finish_code(),
        ("b" | "strong", false) => writer.open_mark("**"),
        ("b" | "strong", true) => writer.close_mark("**"),
        ("i" | "em", false) => writer.open_mark("*"),
        ("i" | "em", true) => writer.close_mark("*"),
        ("s" | "del" | "strike", false) => writer.open_mark("~~"),
        ("s" | "del" | "strike", true) => writer.close_mark("~~"),
        ("a", false) => link_start(writer, tag),
        ("a", true) => link_end(writer),
        ("img", false) => image(writer, tag),
        ("table", false) => writer.start_table(),
        ("table", true) => writer.finish_table(),
        ("tr", false) => writer.start_row(),
        ("tr", true) => writer.finish_cell(),
        ("td" | "th", false) => writer.start_cell(),
        ("td" | "th", true) => writer.finish_cell(),
        _ => {}
    }
}

fn heading_start(writer: &mut Writer, name: &str) {
    writer.block();
    let level = usize::from(name.as_bytes()[1] - b'0');
    if !writer.in_cell() {
        writer.out.push_str(&"#".repeat(level));
        writer.out.push(' ');
    }
    writer.heading = true;
    writer.fresh = true;
}

fn quote_start(writer: &mut Writer) {
    writer.block();
    writer.quote += 1;
    writer.trim_end();
    writer.out.push_str("> ");
}

fn quote_end(writer: &mut Writer) {
    writer.close_marks();
    writer.quote = writer.quote.saturating_sub(1);
    writer.fresh = false;
    writer.block();
}

fn link_start(writer: &mut Writer, tag: &Tag<'_>) {
    let href = attribute(tag.attributes, "href").and_then(|value| safe_url(&value));
    if href.is_some() {
        writer.raw_inline("[");
    }
    writer.links.push(href);
}

fn link_end(writer: &mut Writer) {
    if let Some(Some(href)) = writer.links.pop() {
        writer.trim_end();
        if writer.out.ends_with('[') {
            escape_markdown(&href, &mut writer.out);
        }
        writer.out.push_str(&format!("]({})", link_target(&href)));
    }
}

fn image(writer: &mut Writer, tag: &Tag<'_>) {
    let src = attribute(tag.attributes, "src").unwrap_or_default();
    let alt = attribute(tag.attributes, "alt").unwrap_or_default();
    let image = crate::rich::image_markdown(&src, &alt);
    if !image.is_empty() {
        writer.raw_inline(&image);
    }
}
