use super::{LaunchKind, RedactionContext, RuntimeDetails};

/// Sanitized, immutable opt-in issue fields. Only a local diagnostics snapshot
/// can create this projection; callers cannot inject raw path fields.
#[derive(Debug, Clone)]
pub struct IssuePaths {
    executable: String,
    launch: LaunchKind,
    path_source: String,
    az_path: String,
    config_path: Option<String>,
}

impl IssuePaths {
    pub(super) fn project(
        runtime: Option<&RuntimeDetails>,
        redaction: &RedactionContext,
        config_path: Option<&str>,
    ) -> Self {
        Self {
            executable: redaction.path_field(
                runtime
                    .map(|runtime| runtime.executable.as_str())
                    .unwrap_or("unavailable"),
            ),
            launch: runtime
                .map(|runtime| runtime.launch)
                .unwrap_or(LaunchKind::Unknown),
            path_source: redaction.source(
                runtime
                    .map(|runtime| runtime.path_source.as_str())
                    .unwrap_or("unavailable"),
            ),
            az_path: runtime
                .and_then(|runtime| runtime.az_path.as_deref())
                .map(|path| redaction.path_field(path))
                .unwrap_or_else(|| "not found on the app's PATH".into()),
            config_path: config_path.map(|path| redaction.path_field(path)),
        }
    }

    /// Fixed field names and order. The optional full configuration path is last.
    pub fn fields(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("Executable", self.executable.as_str()),
            ("Launch", self.launch.label()),
            ("PATH source", self.path_source.as_str()),
            ("Azure CLI", self.az_path.as_str()),
        ]
        .into_iter()
        .chain(
            self.config_path
                .as_deref()
                .map(|path| ("Configuration file", path)),
        )
    }
}
