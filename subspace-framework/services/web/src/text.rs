use crate::fetch::BODY_LIMIT;

const TRUNCATION_MARKER: &str = "\n[truncated at 1 MiB]";
const CLOSING_TAG_OVERHEAD: usize = 3;

pub(super) fn accepted_content_type(value: &str) -> Result<&'static str, String> {
    let media_type = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match media_type.as_str() {
        "text/plain" => Ok("text/plain"),
        "text/markdown" => Ok("text/markdown"),
        "text/html" => Ok("text/html"),
        "application/json" => Ok("application/json"),
        "application/xml" => Ok("application/xml"),
        "text/xml" => Ok("text/xml"),
        _ => Err("web_read refuses this content type".into()),
    }
}

pub(super) fn bounded_text(mut body: Vec<u8>) -> (String, bool) {
    let truncated = body.len() > BODY_LIMIT;
    body.truncate(BODY_LIMIT);
    let mut text = String::from_utf8_lossy(&body).into_owned();
    if truncated {
        text.push_str(TRUNCATION_MARKER);
    }
    (text, truncated)
}

pub(super) fn strip_active_content(text: &str, content_type: &str) -> String {
    if !matches!(content_type, "text/html" | "application/xml" | "text/xml") {
        return text.to_owned();
    }
    strip_tags(&remove_element(&remove_element(text, "script"), "style"))
}

fn remove_element(text: &str, element: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut output = String::new();
    let mut position = 0;
    while let Some(relative) = lower[position..].find(&format!("<{element}")) {
        let start = position + relative;
        output.push_str(&text[position..start]);
        let Some(end_relative) = lower[start..].find(&format!("</{element}>")) else {
            return output;
        };
        position = start + end_relative + element.len() + CLOSING_TAG_OVERHEAD;
    }
    output.push_str(&text[position..]);
    output
}

fn strip_tags(text: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}
