//! Safe Markdown link construction.

use url::Url;

use super::literal;

pub(super) fn external_image(url: &str, alt: &str) -> String {
    match Url::parse(url) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => {
            let alt = if alt.trim().is_empty() { "Image" } else { alt };
            ordinary_link(&format!("{alt} ↗"), url.as_str())
        }
        _ => literal(alt),
    }
}

pub(super) fn ordinary_link(label: &str, url: &str) -> String {
    prepared_link(&literal(label), url)
}

pub(super) fn prepared_link(label: &str, url: &str) -> String {
    let destination = url
        .replace('&', "&amp;")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('\\', "%5C")
        .replace(['\n', '\r'], "")
        .replace(' ', "%20");
    format!("[{label}](<{destination}>)")
}
