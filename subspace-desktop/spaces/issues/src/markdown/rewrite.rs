use std::{collections::HashMap, ops::Range};

use markdown::{
    ParseOptions,
    mdast::{Node, ReferenceKind},
};

use super::links::{external_image, ordinary_link, prepared_link};
use super::recovery::{fallback, parse};
const MAX_MEDIA_ID_BYTES: usize = 256;

use super::{Options, format_file_size, literal, literal_label};

struct Edit {
    range: Range<usize>,
    text: String,
}

struct Reader<'a> {
    source: &'a str,
    options: &'a Options<'a>,
    definitions: HashMap<String, String>,
}

pub(super) fn prepare(source: &str, options: &Options<'_>, parser: &ParseOptions) -> String {
    match parse(source, parser) {
        Some((source, tree)) => Reader::new(&source, options, &tree).render(&tree),
        None => fallback(source, parser),
    }
}

impl<'a> Reader<'a> {
    fn new(source: &'a str, options: &'a Options<'a>, tree: &Node) -> Self {
        Self {
            source,
            options,
            definitions: definitions(tree),
        }
    }

    fn render(&self, tree: &Node) -> String {
        let mut edits = Vec::new();
        self.collect(tree, &mut edits);
        apply(self.source, 0..self.source.len(), edits)
    }

    fn collect(&self, node: &Node, edits: &mut Vec<Edit>) {
        let Some(position) = node.position() else {
            return;
        };
        let range = position.start.offset..position.end.offset;
        if matches!(node, Node::Text(_)) {
            edits.extend(bracket_edits(self.source, range, Vec::new()));
            return;
        }
        if let Some(text) = self.replacement(node, range.clone()) {
            edits.push(Edit { range, text });
        } else if let Some(children) = node.children() {
            for child in children {
                self.collect(child, edits);
            }
        }
    }

    fn replacement(&self, node: &Node, range: Range<usize>) -> Option<String> {
        match node {
            // Step 1: protect the entire source, including fences/indentation.
            Node::Code(_) | Node::InlineCode(_) | Node::Definition(_) => {
                Some(self.source[range].into())
            }
            Node::Image(image) => Some(self.image(&image.url, &image.alt)),
            Node::ImageReference(image) => self
                .definitions
                .get(&image.identifier)
                .map(|url| self.image(url, &image.alt)),
            Node::Link(link) => Some(self.link(node, &link.url, range)),
            Node::LinkReference(link) => self
                .definitions
                .get(&link.identifier)
                .map(|url| self.link(node, url, range)),
            _ => None,
        }
    }

    fn image(&self, url: &str, alt: &str) -> String {
        match url.strip_prefix("nett-media:") {
            Some(id) => self.media(id, Some(alt)),
            None => external_image(url, alt),
        }
    }

    fn media(&self, id: &str, alt: Option<&str>) -> String {
        if id.is_empty()
            || id.len() > MAX_MEDIA_ID_BYTES
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return "*Attachment unavailable*".into();
        }
        let Some(media) = self.options.media.iter().find(|media| media.id == id) else {
            return "*Attachment unavailable*".into();
        };
        if let Some(alt) = alt
            && subspace_api_client::media::supported_image(&media.content_type)
        {
            let label = if alt.trim().is_empty() {
                "Image"
            } else {
                alt.trim()
            };
            return format!("[{}](subspace-media:{})", literal(label), media.id);
        }
        format!(
            "[{} ({}, {})](subspace-file:{})",
            literal(&media.name),
            literal(&media.content_type),
            format_file_size(media.size),
            media.id
        )
    }

    fn link(&self, node: &Node, url: &str, range: Range<usize>) -> String {
        let label = visible_label(node);
        if url.split_once(':').is_some_and(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("subspace-media")
                || scheme.eq_ignore_ascii_case("subspace-file")
        }) {
            return literal_label(&label);
        }
        if let Some(id) = url.strip_prefix("nett-media:") {
            return self.media(id, None);
        }
        if let Some(id) = url
            .strip_prefix("member:")
            .filter(|_| mention(self.source, range.start))
        {
            return self.mention(id, &label);
        }
        self.ordinary_link(node, url, &label, range)
    }

    fn mention(&self, id: &str, written: &str) -> String {
        match self.options.mentions.iter().find(|status| status.id == id) {
            // Both membership states are plain text until the reference plugin.
            Some(status) => literal_label(&status.name),
            None => literal_label(written),
        }
    }

    fn ordinary_link(&self, node: &Node, url: &str, label: &str, range: Range<usize>) -> String {
        let raw = &self.source[range.clone()];
        // Step 8: angle and GFM bare autolinks must become ordinary links
        // before escaping '<' and '$' in their visible labels.
        if !raw.starts_with('[') {
            return ordinary_link(label, url);
        }
        let mut edits = Vec::new();
        if let Some(children) = node.children() {
            for child in children {
                self.collect(child, &mut edits);
            }
        }
        // Protect syntax and destinations; only the label is prose.
        let Some(children) = node.children().filter(|children| !children.is_empty()) else {
            return raw.into();
        };
        let start = children[0]
            .position()
            .map_or(range.start, |p| p.start.offset);
        let end = children
            .last()
            .and_then(Node::position)
            .map_or(start, |p| p.end.offset);
        let prepared = apply(self.source, start..end, edits);
        // Resolve the lookup key before escaping its visible label. Keeping
        // shortcut/collapsed source raw would bypass the HTML and math rules.
        if matches!(node, Node::LinkReference(link)
            if matches!(link.reference_kind, ReferenceKind::Shortcut | ReferenceKind::Collapsed))
        {
            return prepared_link(&prepared, url);
        }
        format!(
            "{}{}{}",
            &self.source[range.start..start],
            prepared,
            &self.source[end..range.end]
        )
    }
}

fn bracket_edits(source: &str, range: Range<usize>, mut edits: Vec<Edit>) -> Vec<Edit> {
    for (offset, character) in source[range.clone()].char_indices() {
        let at = range.start + offset;
        if matches!(character, '[' | ']')
            && source[..at]
                .chars()
                .rev()
                .take_while(|c| *c == '\\')
                .count()
                .is_multiple_of(2)
            && !edits.iter().any(|edit| edit.range.contains(&at))
        {
            edits.push(Edit {
                range: at..at + 1,
                text: format!("\\{character}"),
            });
        }
    }
    edits.sort_by_key(|edit| edit.range.start);
    edits
}

fn definitions(tree: &Node) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if let Node::Definition(definition) = node {
            // The first definition wins, as in CommonMark.
            result
                .entry(definition.identifier.clone())
                .or_insert_with(|| definition.url.clone());
        }
        if let Some(children) = node.children() {
            pending.extend(children.iter().rev());
        }
    }
    result
}

fn apply(source: &str, range: Range<usize>, edits: Vec<Edit>) -> String {
    let mut result = String::new();
    let mut cursor = range.start;
    for edit in edits {
        result.push_str(&prose(&source[cursor..edit.range.start]));
        result.push_str(&edit.text);
        cursor = edit.range.end;
    }
    result.push_str(&prose(&source[cursor..range.end]));
    result
}

/// Steps 8 and 9. Existing CommonMark escapes already preserve their character.
fn prose(text: &str) -> String {
    let mut result = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                result.push('\\');
                if let Some(next) = characters.next() {
                    result.push(next);
                }
            }
            '<' => result.push_str("&lt;"),
            '$' => result.push_str("\\$"),
            _ => result.push(character),
        }
    }
    result
}

fn visible_label(node: &Node) -> String {
    match node {
        Node::Break(_) => "\n".into(),
        Node::Image(image) => image.alt.clone(),
        Node::ImageReference(image) => image.alt.clone(),
        _ => match node.children() {
            Some(children) => children.iter().map(visible_label).collect(),
            None => node.to_string(),
        },
    }
}

fn mention(source: &str, start: usize) -> bool {
    let prefix = &source[..start];
    prefix.ends_with('@')
        && prefix[..prefix.len() - 1]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            .is_multiple_of(2)
}
