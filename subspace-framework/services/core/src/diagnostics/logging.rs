use crate::log::{Level, Logger};

use super::{ErrorDetails, LocalDiagnostics};

impl LocalDiagnostics {
    /// Emit one sanitized error record for one accepted occurrence. Owners call
    /// this at the acceptance boundary, never from render or copy. This method
    /// does not deduplicate later identical failures or change logger policy.
    pub fn log_error(&self, logger: &Logger, surface: &str, error: &ErrorDetails) {
        let fields = structured_fields(self, surface, error);
        let borrowed: Vec<_> = fields
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let message = self
            .redaction
            .error_text(&error.title, &error.private_values);
        logger.record(Level::Error, message, &borrowed);
    }
}

fn structured_fields(
    snapshot: &LocalDiagnostics,
    surface: &str,
    error: &ErrorDetails,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![(
        "surface",
        snapshot
            .redaction
            .error_text(surface, &error.private_values),
    )];
    fields.extend(
        super::markdown::error_fields(snapshot, error)
            .into_iter()
            .map(|(_, key, value)| (key, value)),
    );
    fields.push((
        "details",
        snapshot.local_copy(Some(error)).markdown().to_owned(),
    ));
    fields
}
