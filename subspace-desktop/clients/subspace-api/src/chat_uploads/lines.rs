//! How sent references read in IRC: one reference's text, the lines a
//! message and its references become, and finding or hiding them again.
use super::{KEY_LEN, PREVIEW_BYTES, Upload, canonical, word};

/// One reference as the composer sends it: `UPL-… name (N bytes)`, at most
/// `limit` bytes. The name is shortened to fit; the key is never split.
pub fn reference_text(upload: &Upload, limit: usize) -> String {
    let tail = format!(" ({} bytes)", upload.size_bytes);
    let room = limit
        .saturating_sub(upload.reference.len() + 1 + tail.len())
        .min(PREVIEW_BYTES);
    let name = bounded(&upload.file_name, room);
    if name.is_empty() {
        format!("{}{tail}", upload.reference)
    } else {
        format!("{} {name}{tail}", upload.reference)
    }
}

/// The first line of `text` without control characters, within `limit` bytes.
fn bounded(text: &str, limit: usize) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .scan(0, |used, c| {
            *used += c.len_utf8();
            (*used <= limit).then_some(c)
        })
        .collect()
}

/// The IRC lines for `text` and its `references`, each line at most
/// `budget` bytes. Everything goes on one line when it fits; otherwise the
/// text comes first and the references follow in order, packed into as few
/// lines as fit. A reference is never split across lines.
pub fn lines(text: &str, references: &[String], budget: usize) -> Vec<String> {
    let text = text.trim();
    let joined = references.join(" ");
    let single = if text.is_empty() {
        joined.clone()
    } else if joined.is_empty() {
        text.to_owned()
    } else {
        format!("{text} {joined}")
    };
    if !single.contains('\n') && single.len() <= budget {
        return (!single.is_empty()).then_some(single).into_iter().collect();
    }
    let mut lines: Vec<String> = (!text.is_empty())
        .then(|| text.to_owned())
        .into_iter()
        .collect();
    let mut current = String::new();
    for reference in references {
        if !current.is_empty() && current.len() + 1 + reference.len() > budget {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(reference);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Each recognised reference in `text` with the byte range of its sent
/// form: the key, and when one follows before the next key, the name and
/// `(N bytes)` suffix, including the older `UPL-… — preview (N bytes)`.
pub fn reference_spans(text: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let upper = text.to_ascii_uppercase();
    let starts: Vec<usize> = upper
        .match_indices("UPL-")
        .map(|(start, _)| start)
        .filter(|start| {
            let before = start.checked_sub(1).and_then(|i| text.as_bytes().get(i));
            let after = text.as_bytes().get(start + KEY_LEN);
            !(before.is_some_and(word) || after.is_some_and(word))
                && text
                    .get(*start..start + KEY_LEN)
                    .and_then(canonical)
                    .is_some()
        })
        .take(50)
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(at, start)| {
            let key = canonical(text.get(*start..start + KEY_LEN)?)?;
            let window_end = starts.get(at + 1).copied().unwrap_or(text.len());
            let end = suffix_end(text, start + KEY_LEN, window_end).unwrap_or(start + KEY_LEN);
            Some((key, *start..end))
        })
        .collect()
}

/// The end of the first `(N bytes)` between `from` and `to`, on one line.
fn suffix_end(text: &str, from: usize, to: usize) -> Option<usize> {
    let window = text.get(from..to)?;
    let window = &window[..window.find('\n').unwrap_or(window.len())];
    let mut search = 0;
    while let Some(open) = window[search..].find('(') {
        let open = search + open;
        let rest = &window[open + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(" bytes)") {
            return Some(from + open + 1 + digits + " bytes)".len());
        }
        search = open + 1;
    }
    None
}

/// `text` without the sent form of every reference `resolve` knows. A
/// reference's name and `(N bytes)` annotation hides only when it matches
/// the resolved upload's name and size; otherwise only the bare key hides,
/// so prose after a key stays readable.
pub fn without_references<'a>(text: &str, resolve: impl Fn(&str) -> Option<&'a Upload>) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut at = 0;
    for (key, range) in reference_spans(text) {
        let Some(upload) = resolve(&key) else {
            continue;
        };
        let end = if annotated(text, &range, upload) {
            range.end
        } else {
            range.start + KEY_LEN
        };
        shown.push_str(&text[at..range.start]);
        shown.truncate(shown.trim_end_matches([' ', '\t']).len());
        let rest = &text[end..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if !shown.is_empty()
            && !shown.ends_with('\n')
            && !trimmed.is_empty()
            && !trimmed.starts_with('\n')
        {
            shown.push(' ');
        }
        at = text.len() - trimmed.len();
    }
    shown.push_str(&text[at..]);
    shown.trim().to_owned()
}

/// Whether the span's text after its key is exactly what the composer
/// sends for `upload`: its name, whole or shortened, or the older
/// `— name` form, then `(N bytes)` with its real size.
fn annotated(text: &str, range: &std::ops::Range<usize>, upload: &Upload) -> bool {
    let tail = format!("({} bytes)", upload.size_bytes);
    let Some(between) = text
        .get(range.start + KEY_LEN..range.end)
        .and_then(|rest| rest.strip_suffix(tail.as_str()))
    else {
        return false;
    };
    let between = between.trim();
    let named = between
        .strip_prefix("\u{2014}")
        .map_or(between, str::trim_start);
    let name = bounded(&upload.file_name, PREVIEW_BYTES);
    named.is_empty() || name.starts_with(named)
}
