//! How a product binary was built. The product's own build script stamps
//! these values; the Framework never reads a checkout.

/// The build a product binary reports in About, `--version` and
/// diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// The source repository About names; `None` shows no repository line.
    pub repository: Option<&'static str>,
    /// The short commit hash, or `unknown`.
    pub commit: &'static str,
    /// Whether the checkout had uncommitted changes.
    pub dirty: bool,
    pub branch: &'static str,
    /// The UTC build time, such as `2026-11-17T09:30:00Z`.
    pub timestamp: &'static str,
    /// The build time in seconds since the Unix epoch.
    pub unix_time: u64,
    /// The Rust target triple.
    pub target: &'static str,
    /// Cargo's profile, such as `release` or `debug`.
    pub profile: &'static str,
    pub kind: BuildKind,
    /// A semver-safe name for the build machine; `capture` for capture builds.
    pub host: &'static str,
}

/// What a build is for. Every kind but `Release` marks the shown version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Release,
    /// A local build.
    Dev,
    /// A capture build or a test.
    Test,
}

impl BuildKind {
    /// `release`, `dev` or `test`, as build scripts stamp it. Anything else is
    /// a local build.
    pub fn parse(value: &str) -> Self {
        match value {
            "release" => Self::Release,
            "test" => Self::Test,
            _ => Self::Dev,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Dev => "dev",
            Self::Test => "test",
        }
    }
}

impl BuildInfo {
    /// A build no script stamped, as tests and fixtures see it.
    pub const UNSTAMPED: Self = Self {
        repository: None,
        commit: "unknown",
        dirty: false,
        branch: "unknown",
        timestamp: "unknown",
        unix_time: 0,
        target: "unknown",
        profile: "unknown",
        kind: BuildKind::Test,
        host: "unknown",
    };

    /// The commit as reports and About show it: `abc1234` or `abc1234-dirty`.
    pub fn commit_label(&self) -> String {
        if self.dirty {
            format!("{}-dirty", self.commit)
        } else {
            self.commit.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildInfo, BuildKind};

    #[test]
    fn a_dirty_checkout_marks_its_commit() {
        let clean = BuildInfo {
            commit: "abc1234",
            ..BuildInfo::UNSTAMPED
        };
        assert_eq!(clean.commit_label(), "abc1234");
        let dirty = BuildInfo {
            dirty: true,
            ..clean
        };
        assert_eq!(dirty.commit_label(), "abc1234-dirty");
    }

    #[test]
    fn build_kinds_round_trip_and_unknown_values_are_local() {
        for kind in [BuildKind::Release, BuildKind::Dev, BuildKind::Test] {
            assert_eq!(BuildKind::parse(kind.as_str()), kind);
        }
        assert_eq!(BuildKind::parse("nightly"), BuildKind::Dev);
    }
}
