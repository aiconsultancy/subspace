//! Metadata for the GitHub detail panel's footer actions.

use subspace_framework::client_github::model::ItemKind;
use subspace_framework::dashboard::Tone;
use subspace_framework::dashboard::panel::IconName;

use super::Mode;
use super::panel::PanelAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: IconName,
    pub tone: Tone,
    pub action: PanelAction,
}

impl ActionSpec {
    const fn new(
        id: &'static str,
        label: &'static str,
        icon: IconName,
        tone: Tone,
        action: PanelAction,
    ) -> Self {
        Self {
            id,
            label,
            icon,
            tone,
            action,
        }
    }
}

pub(super) fn footer_actions(kind: ItemKind, comment: bool, close: bool) -> Vec<ActionSpec> {
    let mut actions = common_actions(kind, comment);
    if close {
        actions.push(ActionSpec::new(
            "gh-close",
            "Close…  x",
            IconName::GitPullRequestClosed,
            Tone::Red,
            PanelAction::Close,
        ));
    }
    actions
}

fn common_actions(kind: ItemKind, comment: bool) -> Vec<ActionSpec> {
    let mut actions = Vec::new();
    if comment {
        actions.push(ActionSpec::new(
            "gh-comment",
            "Comment  c",
            IconName::MessageSquare,
            Tone::Accent,
            PanelAction::Compose(Mode::Comment),
        ));
    }
    actions.push(ActionSpec::new(
        "gh-assign",
        "Assign  a",
        IconName::UserPlus,
        Tone::Magenta,
        PanelAction::Assign("top"),
    ));
    if kind == ItemKind::Pr {
        actions.push(ActionSpec::new(
            "gh-diff",
            "Files  d",
            IconName::Files,
            Tone::Orange,
            PanelAction::Diff,
        ));
    }
    actions.push(ActionSpec::new(
        "gh-open",
        "GitHub  o",
        IconName::Github,
        Tone::Blue,
        PanelAction::Open,
    ));
    actions
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::ItemKind;
    use subspace_framework::dashboard::Tone;
    use subspace_framework::dashboard::panel::IconName;

    use super::{ActionSpec, Mode, PanelAction, footer_actions};

    #[test]
    fn pull_request_actions_keep_labels_shortcuts_handlers_icons_and_tones() {
        let actions = footer_actions(ItemKind::Pr, true, false);
        let expected = [
            ActionSpec::new(
                "gh-comment",
                "Comment  c",
                IconName::MessageSquare,
                Tone::Accent,
                PanelAction::Compose(Mode::Comment),
            ),
            ActionSpec::new(
                "gh-assign",
                "Assign  a",
                IconName::UserPlus,
                Tone::Magenta,
                PanelAction::Assign("top"),
            ),
            ActionSpec::new(
                "gh-diff",
                "Files  d",
                IconName::Files,
                Tone::Orange,
                PanelAction::Diff,
            ),
            ActionSpec::new(
                "gh-open",
                "GitHub  o",
                IconName::Github,
                Tone::Blue,
                PanelAction::Open,
            ),
        ];

        assert_eq!(actions, expected);
    }
}
