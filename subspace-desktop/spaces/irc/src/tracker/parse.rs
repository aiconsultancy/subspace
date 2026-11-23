//! Reading a draft as a tracker command: `/assign`, `/ready` or
//! `/decision`, its issue and the rest of the line. Pure.

/// A tracker command's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Name {
    Assign,
    Ready,
    Decision,
}

impl Name {
    /// Every tracker command, in the order the completion list offers them.
    pub(crate) const ALL: [Self; 3] = [Self::Assign, Self::Ready, Self::Decision];

    /// The command's word, without its slash. None names an IRC client
    /// command (`subspace_client_irc::COMMANDS`), which a test checks.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Assign => "assign",
            Self::Ready => "ready",
            Self::Decision => "decision",
        }
    }

    /// Its arguments, as the completion list shows them.
    pub(crate) fn args(self) -> &'static str {
        match self {
            Self::Assign => "<issue> [to <agent> | to me]",
            Self::Ready => "<issue> [off]",
            Self::Decision => "<issue> <decision>",
        }
    }

    /// The one-line description beside it. `captain` names the ship whose
    /// captain `/assign` assigns to by default, such as `Lyra`.
    pub(crate) fn description(self, captain: Option<&str>) -> String {
        match (self, captain) {
            (Self::Assign, Some(ship)) => format!("Assign an issue to {ship}"),
            (Self::Assign, None) => "Assign an issue".to_owned(),
            (Self::Ready, _) => "Flag an issue Ready for agents".to_owned(),
            (Self::Decision, _) => "Record a decision on an issue".to_owned(),
        }
    }

    fn named(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|name| name.word().eq_ignore_ascii_case(word))
    }
}

/// A draft read as a tracker command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Typed {
    pub name: Name,
    /// The issue identifier, upper-cased, when one was typed.
    pub issue: Option<String>,
    /// The rest of the line after the issue, trimmed.
    pub rest: String,
}

/// `text` as a tracker command, when it starts with one followed by a
/// space or nothing, in any case. `//assign` is a literal slash.
pub(crate) fn typed(text: &str) -> Option<Typed> {
    let command = text.strip_prefix('/')?;
    let (word, after) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    let name = Name::named(word)?;
    let after = after.trim_start();
    let (issue, rest) = after.split_once(char::is_whitespace).unwrap_or((after, ""));
    Some(Typed {
        name,
        issue: (!issue.is_empty()).then(|| issue.to_ascii_uppercase()),
        rest: rest.trim().to_owned(),
    })
}

/// The command and how many whole arguments come before the word being
/// typed, when `before` (the draft up to that word) is a tracker command
/// and a space.
pub(crate) fn arguments_before(before: &str) -> Option<(Name, Vec<&str>)> {
    let command = before.strip_prefix('/')?;
    if !command.ends_with(char::is_whitespace) {
        return None;
    }
    let mut words = command.split_whitespace();
    let name = Name::named(words.next()?)?;
    Some((name, words.collect()))
}
