use super::{PreparedReport, body_with_diagnostics};

pub const MAX_BROWSER_URL_BYTES: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserMode {
    Prefilled,
    /// Host must copy `clipboard_body` before opening the blank editor.
    ClipboardRequired,
}

/// Previewable instructions, not proof of issue creation or completed I/O.
#[derive(Debug, Clone)]
pub struct BrowserFallback {
    url: String,
    body: String,
    clipboard_body: Option<String>,
    diagnostics_omitted: usize,
}

impl BrowserFallback {
    // qual:allow(dry, boilerplate) reason: "The bounded encoded URL must remain bound to its reviewed effective body."
    pub fn url(&self) -> &str {
        &self.url
    }
    /// Exact prefilled text, or the full text the user must paste.
    // qual:allow(dry, boilerplate) reason: "Readonly preview text prevents divergence from the encoded editor URL."
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn clipboard_body(&self) -> Option<&str> {
        self.clipboard_body.as_deref()
    }
    // qual:api
    // qual:allow(dry, boilerplate) reason: "The host explains diagnostic omission without changing the prepared body."
    pub fn diagnostics_omitted(&self) -> usize {
        self.diagnostics_omitted
    }
    pub fn editor_mode(&self) -> BrowserMode {
        if self.clipboard_body.is_some() {
            BrowserMode::ClipboardRequired
        } else {
            BrowserMode::Prefilled
        }
    }
}

// qual:allow(dry, boilerplate) reason: "The browser variant must own the chosen effective text while the API snapshot remains unchanged."
pub(super) fn prepare(report: &PreparedReport) -> BrowserFallback {
    for keep in (0..=report.diagnostic_lines.len()).rev() {
        let body = body_with_diagnostics(&report.user_body, &report.diagnostic_lines[..keep]);
        let url = encoded_url(&report.repository.new_issue_url(), &report.title, &body);
        if url.len() <= MAX_BROWSER_URL_BYTES {
            return BrowserFallback {
                url,
                body,
                clipboard_body: None,
                diagnostics_omitted: report.diagnostic_lines.len() - keep,
            };
        }
    }
    BrowserFallback {
        url: report.repository.new_issue_url(),
        body: report.body.clone(),
        clipboard_body: Some(report.body.clone()),
        diagnostics_omitted: 0,
    }
}

fn encoded_url(editor: &str, title: &str, body: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("title", title)
        .append_pair("body", body)
        .finish();
    format!("{editor}&{query}")
}
