use chrono::{DateTime, Utc};

/// Error facts supplied by the error owner, not collected from logs or state.
#[derive(Clone)]
pub struct ErrorDetails {
    pub(super) title: String,
    pub(super) message: String,
    pub(super) stage: String,
    pub(super) code: Option<String>,
    pub(super) chain: Option<String>,
    pub(super) time: ErrorTime,
    pub(super) private_values: Vec<String>,
}

#[derive(Clone)]
pub(super) enum ErrorTime {
    Recorded(DateTime<Utc>),
    Observed(DateTime<Utc>),
}

// qual:allow(dry, boilerplate) reason: "Error owners supply optional recorded facts and private removal inputs through a small private-field API, without a new builder dependency or serializable state."
impl ErrorDetails {
    /// Legacy errors without a source timestamp are observed now, not backdated.
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        stage: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            stage: stage.into(),
            code: None,
            chain: None,
            time: ErrorTime::Observed(Utc::now()),
            private_values: Vec::new(),
        }
    }

    pub fn with_recorded_at(mut self, at: DateTime<Utc>) -> Self {
        self.time = ErrorTime::Recorded(at);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_observed_at(mut self, at: DateTime<Utc>) -> Self {
        self.time = ErrorTime::Observed(at);
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Include the available underlying chain alongside a friendly code.
    /// Copy and logging apply the same private-value redaction and text bound.
    pub fn with_chain(mut self, chain: impl Into<String>) -> Self {
        self.chain = Some(chain.into());
        self
    }

    /// Append target/resource/draft values used only for removal from error
    /// text. These inputs are never rendered, logged or serialized. Matching
    /// is literal and case-sensitive, with identifier boundaries to preserve
    /// ordinary words.
    pub fn with_private_values(mut self, values: impl IntoIterator<Item = String>) -> Self {
        self.private_values.extend(values);
        super::redact::normalize_values(&mut self.private_values);
        self
    }
}
