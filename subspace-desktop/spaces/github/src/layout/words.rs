//! The words of the status line, the navigator's footer and the view
//! counts: what the Space knows about the account, and how each view's last
//! load went. Pure, so the tests run them without a window.

/// What the Space knows about the signed-in GitHub account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signed<'a> {
    /// GitHub could not be read.
    Unavailable,
    /// Signed in with this login.
    In(&'a str),
    /// The accounts are loading.
    Checking,
    /// Before the Space first loads.
    Unknown,
}

impl Signed<'_> {
    /// The context at the start of the status line: the service, then the
    /// account.
    pub(crate) fn scope_line(self) -> String {
        match self {
            Self::Unavailable => "GitHub · Account unavailable".into(),
            Self::In(login) => format!("GitHub · Signed in as {login}"),
            Self::Checking => "GitHub · Checking account…".into(),
            Self::Unknown => "GitHub".into(),
        }
    }

    /// The line in the navigator's footer.
    pub(crate) fn footer_line(self) -> String {
        match self {
            Self::Unavailable => "GitHub account unavailable".into(),
            Self::In(login) => format!("Signed in to GitHub as {login}"),
            Self::Checking => "Checking GitHub account…".into(),
            Self::Unknown => "GitHub".into(),
        }
    }
}

/// How a view's last load went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Load {
    Ready,
    Loading,
    Failed,
}

/// A view's count: the total GitHub reports, once a list has loaded, and how
/// the last load went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewCount {
    pub(crate) total: Option<u32>,
    pub(crate) load: Load,
}

impl ViewCount {
    /// The count at the end of the view's navigator row. A load in progress
    /// adds `…` and a failed one `!`, so the state does not rest on colour.
    pub(crate) fn mark(self) -> String {
        match (self.total, self.load) {
            (Some(total), Load::Ready) => total.to_string(),
            (Some(total), Load::Loading) => format!("{total} …"),
            (Some(total), Load::Failed) => format!("{total} !"),
            (None, Load::Failed) => "!".into(),
            (None, _) => "…".into(),
        }
    }

    /// The same in words, for the view menu.
    pub(crate) fn words(self) -> String {
        match (self.total, self.load) {
            (Some(total), Load::Ready) => format!("{total} items"),
            (Some(total), Load::Loading) => format!("{total} items, loading"),
            (Some(total), Load::Failed) => format!("{total} items, load failed"),
            (None, Load::Failed) => "Load failed".into(),
            (None, _) => "loading".into(),
        }
    }
}

/// The current view's counts, such as `8 open PRs · 4 repos`, or
/// `50 of 73 open PRs · …` when GitHub has more than the list loaded.
pub(crate) fn counts_line(shown: usize, total: u32, repos: usize, noun: &str) -> String {
    let count = if total as usize > shown {
        format!("{shown} of {total}")
    } else {
        total.to_string()
    };
    let unit = if repos == 1 { "repo" } else { "repos" };
    format!("{count} {noun} · {repos} {unit}")
}

/// The status line: the account context, then the view's counts once its
/// list has loaded.
pub(crate) fn status_line(scope: &str, counts: Option<&str>) -> String {
    match counts {
        Some(counts) => format!("{scope} · {counts}"),
        None => scope.to_owned(),
    }
}
