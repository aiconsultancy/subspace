use super::{Error, ErrorKind};

const MAX_TITLE_CHARS: usize = 256;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IssueType {
    Bug,
    Ux,
    FeatureRequest,
    Other,
    #[default]
    Unknown,
}

impl IssueType {
    pub fn label(self) -> &'static str {
        self.descriptor().1
    }

    pub fn name(self) -> &'static str {
        self.descriptor().0
    }

    fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::Bug => ("Bug", "bug"),
            Self::Ux => ("UX", "ux"),
            Self::FeatureRequest => ("Feature request", "enhancement"),
            Self::Other => ("Other", "other"),
            Self::Unknown => ("Unknown", "unknown"),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Draft {
    title: String,
    description: String,
    issue_type: Option<IssueType>,
    steps: String,
}

// qual:allow(dry, boilerplate) reason: "Native builders preserve the required private-field reporting API without adding a derive dependency."
impl Draft {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            ..Self::default()
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_issue_type(mut self, issue_type: IssueType) -> Self {
        self.issue_type = Some(issue_type);
        self
    }

    pub fn with_steps(mut self, steps: impl Into<String>) -> Self {
        self.steps = steps.into();
        self
    }

    // qual:allow(dry, boilerplate) reason: "The form's draft text is exposed through readers and replacement builders, not mutable public fields."
    pub fn title(&self) -> &str {
        &self.title
    }

    // qual:allow(dry, boilerplate) reason: "The host reads user text without bypassing the replacement-builder boundary."
    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn issue_type(&self) -> IssueType {
        self.issue_type.unwrap_or_default()
    }

    // qual:allow(dry, boilerplate) reason: "The host reads exact reproduction text without mutating a prepared snapshot."
    pub fn steps(&self) -> &str {
        &self.steps
    }
}

pub(super) fn validated_title(draft: &Draft) -> Result<String, Error> {
    let first_line = draft
        .description
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or(ErrorKind::DescriptionRequired)?;
    let title = draft.title.trim();
    if title.is_empty() {
        let derived: String = first_line.trim().chars().take(MAX_TITLE_CHARS).collect();
        if derived.chars().any(char::is_control) {
            return Err(ErrorKind::InvalidTitle.into());
        }
        return Ok(derived);
    }
    if title.chars().count() > MAX_TITLE_CHARS || title.chars().any(char::is_control) {
        return Err(ErrorKind::InvalidTitle.into());
    }
    Ok(title.into())
}

pub(super) fn body(draft: &Draft) -> String {
    let mut body = format!(
        "## Description\n\n{}\n\n## Type\n\n{}",
        draft.description,
        draft.issue_type().name()
    );
    if !draft.steps.is_empty() {
        body.push_str(&format!("\n\n## Steps to reproduce\n\n{}", draft.steps));
    }
    body
}
