//! Reviewed issue reports for the repository the app names. No UI or image
//! I/O.

mod browser;
mod diagnostics;
mod draft;
mod error;
mod http;
mod repository;
mod submission;

pub use browser::{BrowserFallback, BrowserMode, MAX_BROWSER_URL_BYTES};
pub use diagnostics::Diagnostics;
pub use draft::{Draft, IssueType};
pub use error::{Error, ErrorKind};
pub use repository::IssueRepository;
pub use submission::{CreatedIssue, Outcome, ReportPlan, TransportKind};
pub use subspace_core::diagnostics::IssuePaths;

const MAX_BODY_CHARS: usize = 65_536;

/// Immutable text snapshot. Editing the form requires preparing a new snapshot.
#[derive(Debug, Clone)]
// qual:allow(srp, god_struct) reason: "One validated report snapshot owns its title, exact body, type and browser fallback projection together."
pub struct PreparedReport {
    title: String,
    body: String,
    user_body: String,
    diagnostic_lines: Vec<String>,
    issue_type: IssueType,
    product_id: String,
    repository: IssueRepository,
}

impl PreparedReport {
    // qual:allow(dry, boilerplate) reason: "The validated title must remain immutable after user review."
    pub fn title(&self) -> &str {
        &self.title
    }

    // qual:allow(dry, boilerplate) reason: "The exact reviewed body cannot be replaced independently of its fallback projection."
    pub fn body(&self) -> &str {
        &self.body
    }

    // qual:allow(dry, boilerplate) reason: "The type remains bound to the reviewed Markdown and requested labels."
    pub fn issue_type(&self) -> IssueType {
        self.issue_type
    }

    /// The repository the report files into.
    // qual:allow(dry, boilerplate) reason: "The repository stays bound to the reviewed report it was prepared for."
    pub fn repository(&self) -> &IssueRepository {
        &self.repository
    }

    pub fn requested_labels(&self) -> [String; 2] {
        [
            self.issue_type.label().into(),
            format!("source:{}", self.product_id),
        ]
    }

    pub fn browser_fallback(&self) -> BrowserFallback {
        browser::prepare(self)
    }
}

/// Validate form fields and construct the exact API title and Markdown body
/// of a report into `repository`.
pub fn prepare(
    draft: &Draft,
    diagnostics: &Diagnostics,
    repository: &IssueRepository,
) -> Result<PreparedReport, Error> {
    let title = draft::validated_title(draft)?;
    let diagnostic_lines = diagnostics::lines(diagnostics)?;
    let user_body = draft::body(draft);
    let body = body_with_diagnostics(&user_body, &diagnostic_lines);
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(ErrorKind::BodyTooLong.into());
    }
    Ok(PreparedReport {
        title,
        body,
        user_body,
        diagnostic_lines,
        issue_type: draft.issue_type(),
        product_id: diagnostics.product_id().into(),
        repository: repository.clone(),
    })
}

fn body_with_diagnostics(user_body: &str, lines: &[String]) -> String {
    if lines.is_empty() {
        return user_body.into();
    }
    format!("{user_body}\n\n## Diagnostics\n\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests;
