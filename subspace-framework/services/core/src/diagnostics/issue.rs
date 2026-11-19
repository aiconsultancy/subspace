use super::{BuildIdentity, IssuePaths};

/// Issue-only allowlist. Paths require a separately sanitized, explicit opt-in
/// projection; this type cannot receive local errors, logs or runtime state.
#[derive(Debug, Clone)]
pub struct IssueDiagnostics {
    identity: BuildIdentity,
    active_board: Option<String>,
    theme_id: Option<String>,
    icon_size: Option<String>,
    paths: Option<IssuePaths>,
}

// qual:allow(dry, boilerplate) reason: "The issue projection accepts shared registered identities and explicitly excludes every local field; no conversion dependency is needed for this small allowlist."
impl From<BuildIdentity> for IssueDiagnostics {
    fn from(identity: BuildIdentity) -> Self {
        Self {
            identity,
            active_board: None,
            theme_id: None,
            icon_size: None,
            paths: None,
        }
    }
}

// qual:allow(dry, boilerplate) reason: "Preserve the existing issue Diagnostics builder API while keeping its fields private and its allowlist unchanged; a builder dependency would add no behavior."
impl IssueDiagnostics {
    pub fn new(product_id: impl Into<String>, app_version: impl Into<String>) -> Self {
        BuildIdentity::new(product_id, app_version).into()
    }

    pub fn with_build(mut self, channel: impl Into<String>, commit: impl Into<String>) -> Self {
        self.identity = self.identity.with_build(channel, commit);
        self
    }

    pub fn with_platform(mut self, os: impl Into<String>, architecture: impl Into<String>) -> Self {
        self.identity = self.identity.with_platform(os, architecture);
        self
    }

    pub fn with_active_board(mut self, id: impl Into<String>) -> Self {
        self.active_board = Some(id.into());
        self
    }

    pub fn with_appearance(
        mut self,
        theme_id: impl Into<String>,
        icon_size: impl Into<String>,
    ) -> Self {
        self.theme_id = Some(theme_id.into());
        self.icon_size = Some(icon_size.into());
        self
    }

    pub fn with_paths(mut self, paths: IssuePaths) -> Self {
        self.paths = Some(paths);
        self
    }

    pub fn without_paths(mut self) -> Self {
        self.paths = None;
        self
    }

    pub fn paths(&self) -> Option<&IssuePaths> {
        self.paths.as_ref()
    }

    pub fn product_id(&self) -> &str {
        self.identity.product_id()
    }

    pub fn app_version(&self) -> &str {
        self.identity.app_version()
    }

    pub fn channel(&self) -> Option<&str> {
        self.identity.channel()
    }

    pub fn commit(&self) -> Option<&str> {
        self.identity.commit()
    }

    pub fn os(&self) -> Option<&str> {
        self.identity.os()
    }

    pub fn architecture(&self) -> Option<&str> {
        self.identity.architecture()
    }

    pub fn active_board(&self) -> Option<&str> {
        self.active_board.as_deref()
    }

    pub fn theme_id(&self) -> Option<&str> {
        self.theme_id.as_deref()
    }

    /// Semantic global size key, not physical pixels or a resolved glyph size.
    pub fn icon_size(&self) -> Option<&str> {
        self.icon_size.as_deref()
    }
}
