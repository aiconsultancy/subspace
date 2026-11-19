/// Launch classification captured by the shell, without spawning a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    Terminal,
    Desktop,
    Unknown,
}

impl LaunchKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Desktop => "desktop",
            Self::Unknown => "unknown",
        }
    }
}

/// Named local runtime fields only. There is deliberately no full PATH field.
#[derive(Clone)]
pub struct RuntimeDetails {
    pub(super) executable: String,
    pub(super) launch: LaunchKind,
    pub(super) path_source: String,
    pub(super) az_path: Option<String>,
}

impl RuntimeDetails {
    pub fn new(
        executable: impl Into<String>,
        launch: LaunchKind,
        path_source: impl Into<String>,
        az_path: Option<String>,
    ) -> Self {
        Self {
            executable: executable.into(),
            launch,
            path_source: path_source.into(),
            az_path,
        }
    }
}
