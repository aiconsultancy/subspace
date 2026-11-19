use super::{BuildIdentity, ErrorDetails, IssuePaths, RedactionContext, RuntimeDetails};

/// Shell-owned snapshot. Collection happens once outside render and copy.
/// Raw private inputs have no serialization or Debug implementation.
#[derive(Clone)]
pub struct LocalDiagnostics {
    pub(super) identity: BuildIdentity,
    pub(super) framework_version: String,
    pub(super) os_version: Option<String>,
    pub(super) runtime: Option<RuntimeDetails>,
    pub(super) redaction: RedactionContext,
}

impl Default for LocalDiagnostics {
    fn default() -> Self {
        Self::new(
            BuildIdentity::new("unavailable", "unavailable"),
            "unavailable",
        )
    }
}

// qual:allow(dry, boilerplate) reason: "The shell injects optional local facts and a private non-serializable redaction context; the builders keep raw inputs out of the public record schema without a new dependency."
impl LocalDiagnostics {
    pub fn new(identity: BuildIdentity, framework_version: impl Into<String>) -> Self {
        Self {
            identity,
            framework_version: framework_version.into(),
            os_version: None,
            runtime: None,
            redaction: RedactionContext::default(),
        }
    }

    pub fn with_os_version(mut self, version: impl Into<String>) -> Self {
        self.os_version = Some(version.into());
        self
    }

    pub fn with_runtime(mut self, runtime: RuntimeDetails) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_redaction(mut self, redaction: RedactionContext) -> Self {
        self.redaction = redaction;
        self
    }

    /// Project only the explicit per-report path allowlist, without error text
    /// or the additional fields included by local copy.
    pub fn issue_paths(&self, config_path: Option<&str>) -> IssuePaths {
        IssuePaths::project(self.runtime.as_ref(), &self.redaction, config_path)
    }

    pub fn local_copy(&self, error: Option<&ErrorDetails>) -> LocalCopy {
        let fields = [
            ("Product", Some(self.identity.product_id())),
            ("App version", Some(self.identity.app_version())),
            ("Build channel", self.identity.channel()),
            ("Commit", self.identity.commit()),
            (
                "Subspace Framework version",
                Some(self.framework_version.as_str()),
            ),
            ("OS", self.identity.os()),
            ("OS version", self.os_version.as_deref()),
            ("Architecture", self.identity.architecture()),
        ];
        let mut fields: Vec<_> = fields
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    self.redaction.public_field(value.unwrap_or("unavailable")),
                )
            })
            .collect();
        fields.extend(super::markdown::runtime_fields(
            self.runtime.as_ref(),
            &self.redaction,
        ));
        let diagnostics = super::markdown::section("Diagnostics", &fields);
        LocalCopy(super::markdown::render(self, error, diagnostics))
    }
}

/// Sanitized local-copy projection ready for clipboard delivery. Free-text
/// values are flattened to one line and capped at 4,096 Unicode characters
/// after redaction, with a visible truncation marker.
#[derive(Debug, Clone)]
pub struct LocalCopy(String);

impl LocalCopy {
    pub fn markdown(&self) -> &str {
        &self.0
    }
}
