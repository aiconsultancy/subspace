//! Labelled plain text for an attached chat reference.
//!
//! A reference is data, not instructions, so it reads as `Label: value`
//! lines and headed blocks, never as Rust debug output. Every builder here
//! is pure, so each dashboard can test the text it attaches without a
//! window. [`cap`] bounds one reference at [`MAX_CONTEXT_BYTES`] and says so
//! in the text when it cuts.

use subspace_agent_model::ContextRef;

use crate::html;
use crate::list::{Body, Detail};

/// The most text one attached reference carries.
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;

/// Appended when a reference reaches [`MAX_CONTEXT_BYTES`].
pub const TRUNCATION_MARKER: &str = "\n… truncated: this reference reached the 16 KB limit.";

/// A reference's text, built line by line.
#[derive(Debug, Default, Clone)]
pub struct ContextText {
    lines: Vec<String>,
}

impl ContextText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `Label: value`. A blank value adds nothing, so an absent field
    /// never shows as an empty label.
    pub fn line(&mut self, label: &str, value: impl AsRef<str>) {
        let value = value.as_ref().trim();
        if !value.is_empty() {
            self.lines.push(format!("{label}: {value}"));
        }
    }

    /// [`ContextText::line`] for an optional value.
    pub fn option<S: AsRef<str>>(&mut self, label: &str, value: Option<S>) {
        self.line(label, value.as_ref().map_or("", AsRef::as_ref));
    }

    /// A headed block that keeps its own line breaks, such as a description.
    pub fn block(&mut self, heading: &str, body: impl AsRef<str>) {
        let body = body.as_ref().trim();
        if !body.is_empty() {
            self.lines.push(format!("\n{heading}:\n{body}"));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The text, capped at [`MAX_CONTEXT_BYTES`].
    pub fn finish(&self) -> String {
        cap(self.lines.join("\n"))
    }
}

/// `text` within [`MAX_CONTEXT_BYTES`], cut on a character boundary and
/// marked when it was cut.
pub fn cap(text: String) -> String {
    cap_to(text, MAX_CONTEXT_BYTES, TRUNCATION_MARKER)
}

/// `text` within `limit` bytes, cut on a character boundary and marked when cut.
pub fn cap_to(text: String, limit: usize, marker: &str) -> String {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit.saturating_sub(marker.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{marker}", &text[..end])
}

/// A section's text; service HTML becomes Markdown, as the panel shows it.
pub fn body_text(body: &Body) -> String {
    match body {
        Body::Text(text) | Body::Markdown(text) => text.to_string(),
        Body::Html(html) => html::to_markdown(html),
    }
}

/// Everything the shared detail panel shows for one row: its head, its
/// properties, then its sections.
// qual:api
pub fn add_detail(text: &mut ContextText, detail: &Detail) {
    add_detail_head(text, detail);
    add_detail_props(text, detail);
    add_detail_sections(text, detail);
}

fn add_detail_head(text: &mut ContextText, detail: &Detail) {
    text.line("Type", &detail.kind);
    text.line("Id", &detail.id);
    text.line("Title", &detail.title);
    text.option("Link", detail.link.as_ref().map(|link| link.url.as_ref()));
}

fn add_detail_props(text: &mut ContextText, detail: &Detail) {
    for (label, value) in &detail.props {
        text.line(label, value);
    }
}

fn add_detail_sections(text: &mut ContextText, detail: &Detail) {
    for section in &detail.sections {
        text.block(&section.heading, body_text(&section.body));
    }
}

/// A reference, with its content capped.
pub fn reference(key: String, label: String, content: String) -> ContextRef {
    ContextRef {
        key,
        label,
        content: cap(content),
    }
}

/// A reference for text selected in a detail pane. `source` names where it
/// came from, such as `GitHub: Fix login`. Blank selections give `None`.
pub fn selection_reference(source: &str, selected: &str) -> Option<ContextRef> {
    let selected = selected.trim();
    if selected.is_empty() {
        return None;
    }
    let mut text = ContextText::new();
    text.line("Selected text from", source);
    text.block("Selection", selected);
    Some(reference(
        format!("selection:{source}:{}", fingerprint(selected)),
        format!("Selection · {source}"),
        text.finish(),
    ))
}

/// A short stable digest (FNV-1a), so the same selection attaches once.
fn fingerprint(text: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{
        ContextText, MAX_CONTEXT_BYTES, TRUNCATION_MARKER, add_detail, body_text, cap, cap_to,
        selection_reference,
    };
    use crate::list::{Body, Detail, Section, ServiceLink};

    fn detail_text(detail: &Detail) -> String {
        let mut text = ContextText::new();
        add_detail(&mut text, detail);
        text.finish()
    }

    #[test]
    fn blank_values_add_no_line() {
        let mut text = ContextText::new();
        text.line("Repository", "octo/app");
        text.line("Branch", "  ");
        text.option("File", None::<&str>);
        text.option("Rule", Some("S101"));
        text.block("Description", "   ");
        assert_eq!(text.finish(), "Repository: octo/app\nRule: S101");
    }

    #[test]
    fn a_block_keeps_its_own_lines_under_a_heading() {
        let mut text = ContextText::new();
        text.line("Title", "Fix it");
        text.block("Description", "one\ntwo");
        assert_eq!(text.finish(), "Title: Fix it\n\nDescription:\none\ntwo");
    }

    #[test]
    fn long_text_is_cut_on_a_boundary_and_marked() {
        let capped = cap("é".repeat(MAX_CONTEXT_BYTES).to_string());
        assert!(capped.len() <= MAX_CONTEXT_BYTES + TRUNCATION_MARKER.len());
        assert!(capped.ends_with(TRUNCATION_MARKER));
        let short = cap("kept".to_string());
        assert_eq!(short, "kept");
        assert_eq!(cap_to("ééé".into(), 5, "!"), "éé!");
    }

    #[test]
    fn service_html_reads_as_markdown() {
        assert_eq!(body_text(&Body::Text("plain".into())), "plain");
        let markdown = body_text(&Body::Html("<p>Holds <b>PII</b></p>".into()));
        assert!(markdown.contains("PII"));
        assert!(!markdown.contains("<b>"));
    }

    #[test]
    fn a_detail_reads_as_its_panel_shows_it() {
        let detail = Detail {
            kind: "Pull request".into(),
            id: "octo/app#12".into(),
            title: "Port the board".into(),
            link: Some(ServiceLink::new(
                "GitHub ↗",
                "https://github.com/octo/app/pull/12",
            )),
            props: vec![
                ("State".into(), "Open".into()),
                ("Checks".into(), "3 passing".into()),
            ],
            sections: vec![Section::markdown("Body", "Ports the board view.")],
        };
        let text = detail_text(&detail);
        assert!(text.contains("Type: Pull request"));
        assert!(text.contains("Id: octo/app#12"));
        assert!(text.contains("State: Open"));
        assert!(text.contains("Checks: 3 passing"));
        assert!(text.contains("\nBody:\nPorts the board view."));
        assert!(!text.contains("SharedString"));
    }

    #[test]
    fn a_selection_names_its_source_and_keys_by_content() {
        let first = selection_reference("GitHub: Fix login", " select 1 \n").expect("a reference");
        assert_eq!(first.label, "Selection · GitHub: Fix login");
        assert!(
            first
                .content
                .contains("Selected text from: GitHub: Fix login")
        );
        assert!(first.content.contains("select 1"));
        let same = selection_reference("GitHub: Fix login", "select 1").expect("a reference");
        assert_eq!(first.key, same.key);
        let other = selection_reference("GitHub: Fix login", "select 2").expect("a reference");
        assert_ne!(first.key, other.key);
        assert!(selection_reference("GitHub", "  ").is_none());
    }
}
