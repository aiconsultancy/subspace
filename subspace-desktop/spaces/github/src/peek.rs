//! What the inspector and the pull request page show, as plain data: the
//! description's excerpt, the facts, the reviewers' and checks' status rows,
//! and which actions the inspector's footer offers. The render code in
//! `inspector` and `page` draws these; the tests run them without a window.

use subspace_framework::client_github::model::{
    GhCheck, GhItem, GhItemDetail, ItemKind, display_name,
};
use subspace_framework::dashboard::Glyph;
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::panel::IconName;

use crate::close_pull_request::is_closable;
use crate::detail::{Mode, PanelAction};
use crate::model::{CHECKS_SHOWN, age, can_comment, can_review, is_failed_check};
use crate::rows::{check_glyph, reviewer_badge};

/// The excerpt stops at about this many characters, on a word.
const EXCERPT_CHARS: usize = 280;
/// Assignees past this many collapse into "…".
const ASSIGNEES_SHOWN: usize = 4;

/// The description's first paragraph as plain text, cut to a few lines.
/// `None` when the description is empty.
pub(crate) fn excerpt(html: &str) -> Option<String> {
    let paragraph = html
        .split("</p>")
        .map(plain_text)
        .find(|text| !text.is_empty())?;
    Some(cut(&paragraph, EXCERPT_CHARS))
}

/// The text of an HTML fragment: tags dropped, the common entities decoded,
/// and runs of white space made one space.
fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` cut to `limit` characters at the last word that fits, with "…".
fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let head: String = text.chars().take(limit).collect();
    let words = match head.rfind(' ') {
        Some(space) => &head[..space],
        None => head.as_str(),
    };
    format!("{}…", words.trim_end())
}

/// One labelled fact in the inspector's Properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fact {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    /// Drawn in the monospace font, as a branch name is.
    pub(crate) mono: bool,
}

impl Fact {
    fn new(label: &'static str, value: String) -> Self {
        Self {
            label,
            value,
            mono: false,
        }
    }
}

/// Author, Branch, Files and Updated, then Assignees and Labels when the
/// pull request has any.
pub(crate) fn facts(item: &GhItem, detail: Option<&GhItemDetail>, now: i64) -> Vec<Fact> {
    let author = item.author.as_ref().map_or_else(
        || "Unknown".to_owned(),
        |user| display_name(user).to_owned(),
    );
    let mut facts = vec![Fact::new("Author", author)];
    if let (Some(head), Some(base)) = (&item.head_ref, &item.base_ref) {
        facts.push(Fact {
            mono: true,
            ..Fact::new("Branch", format!("{head} → {base}"))
        });
    }
    if let Some(files) = files_text(item, detail) {
        facts.push(Fact::new("Files", files));
    }
    facts.push(Fact::new(
        "Updated",
        format!("{} ago", age(&item.updated_at, now)),
    ));
    if !item.assignees.is_empty() {
        facts.push(Fact::new("Assignees", assignees_text(item)));
    }
    if !item.labels.is_empty() {
        let labels: Vec<&str> = item
            .labels
            .iter()
            .map(|label| label.name.as_str())
            .collect();
        facts.push(Fact::new("Labels", labels.join(", ")));
    }
    facts
}

/// `3 files · +53 −7`, or the line counts alone until the details load.
fn files_text(item: &GhItem, detail: Option<&GhItemDetail>) -> Option<String> {
    let files = match detail.and_then(|detail| detail.changed_files) {
        Some(1) => Some("1 file".to_owned()),
        Some(count) => Some(format!("{count} files")),
        None => None,
    };
    let stat = item
        .additions
        .zip(item.deletions)
        .map(|(added, deleted)| format!("+{added} −{deleted}"));
    match (files, stat) {
        (Some(files), Some(stat)) => Some(format!("{files} · {stat}")),
        (files, stat) => files.or(stat),
    }
}

fn assignees_text(item: &GhItem) -> String {
    let mut names: Vec<&str> = item
        .assignees
        .iter()
        .take(ASSIGNEES_SHOWN)
        .map(display_name)
        .collect();
    if item.assignees.len() > ASSIGNEES_SHOWN {
        names.push("…");
    }
    names.join(", ")
}

/// A reviewer or a check: a glyph whose shape tells the status on its own,
/// a name, the status word, and the check's page when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusRow {
    pub(crate) glyph: Glyph,
    pub(crate) name: String,
    pub(crate) word: &'static str,
    pub(crate) url: Option<String>,
}

/// One row per reviewer, in GitHub's order.
pub(crate) fn reviewer_rows(detail: &GhItemDetail) -> Vec<StatusRow> {
    detail
        .reviewers
        .iter()
        .map(|reviewer| {
            let (glyph, word) = reviewer_badge(reviewer.state);
            StatusRow {
                glyph,
                name: display_name(&reviewer.user).to_owned(),
                word,
                url: None,
            }
        })
        .collect()
}

/// One row per check. Above `CHECKS_SHOWN` checks, only the failing ones
/// unless `show_all`.
pub(crate) fn check_rows(checks: &[GhCheck], show_all: bool) -> Vec<StatusRow> {
    let collapsed = checks.len() > CHECKS_SHOWN && !show_all;
    checks
        .iter()
        .filter(|check| !collapsed || is_failed_check(check))
        .map(|check| {
            let (glyph, word) = check_glyph(check.state);
            StatusRow {
                glyph,
                name: check.name.clone(),
                word,
                url: check.url.clone(),
            }
        })
        .collect()
}

/// An action on the pull request the inspector shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemAction {
    Approve,
    RequestChanges,
    Comment,
    Assign,
    Files,
    Close,
}

/// What the account may do to a pull request, as the old panel decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Allowed {
    pub(crate) review: bool,
    pub(crate) comment: bool,
    pub(crate) files: bool,
    pub(crate) close: bool,
}

impl Allowed {
    pub(crate) fn of(item: &GhItem, detail: Option<&GhItemDetail>) -> Self {
        Self {
            review: can_review(item, detail),
            comment: can_comment(detail),
            files: item.kind == ItemKind::Pr,
            close: is_closable(item),
        }
    }
}

/// The inspector footer's actions, in order. An action the account may
/// not take is left out, as the old panel's bar left it out; Assign is
/// always there.
pub(crate) fn footer_actions(allowed: Allowed) -> Vec<ItemAction> {
    let mut actions = review_actions(allowed);
    actions.push(ItemAction::Assign);
    if allowed.files {
        actions.push(ItemAction::Files);
    }
    if allowed.close {
        actions.push(ItemAction::Close);
    }
    actions
}

/// The review actions alone, for the page's toolbar.
pub(crate) fn review_actions(allowed: Allowed) -> Vec<ItemAction> {
    let mut actions = Vec::new();
    if allowed.review {
        actions.extend([ItemAction::Approve, ItemAction::RequestChanges]);
    }
    if allowed.comment {
        actions.push(ItemAction::Comment);
    }
    actions
}

/// How an action's button looks and what it asks of the Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionLook {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    /// The spoken name, where it says more than the label.
    pub(crate) name: &'static str,
    pub(crate) icon: IconName,
    pub(crate) intent: Intent,
    pub(crate) action: PanelAction,
}

/// Approve is the one primary action; the rest are neutral.
pub(crate) fn look(action: ItemAction) -> ActionLook {
    let (id, label, name, icon, panel) = match action {
        ItemAction::Approve => (
            "gh-approve",
            "Approve  A",
            "Approve",
            IconName::Check,
            PanelAction::Compose(Mode::Approve),
        ),
        ItemAction::RequestChanges => (
            "gh-changes",
            "Request changes  R",
            "Request changes",
            IconName::ShieldAlert,
            PanelAction::Compose(Mode::Changes),
        ),
        ItemAction::Comment => (
            "gh-comment",
            "Comment  c",
            "Comment",
            IconName::MessageSquare,
            PanelAction::Compose(Mode::Comment),
        ),
        ItemAction::Assign => (
            "gh-assign",
            "Assign  a",
            "Assign",
            IconName::UserPlus,
            PanelAction::Assign("top"),
        ),
        ItemAction::Files => (
            "gh-diff",
            "Files  d",
            "Files: show the diff",
            IconName::Files,
            PanelAction::Diff,
        ),
        ItemAction::Close => (
            "gh-close",
            "Close…  x",
            "Close pull request…",
            IconName::GitPullRequestClosed,
            PanelAction::Close,
        ),
    };
    let intent = match action {
        ItemAction::Approve => Intent::Primary,
        _ => Intent::Neutral,
    };
    ActionLook {
        id,
        label,
        name,
        icon,
        intent,
        action: panel,
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::{
        CheckRunState, GhCheck, GhItemDetail, GhReviewer, GhUser, ItemKind, ReviewerState,
    };
    use subspace_framework::dashboard::action::Intent;

    use super::{
        Allowed, ItemAction, check_rows, excerpt, facts, footer_actions, look, review_actions,
        reviewer_rows,
    };
    use crate::sample;

    fn detail() -> GhItemDetail {
        let mut item = sample::items().remove(0);
        item.kind = ItemKind::Pr;
        GhItemDetail {
            item,
            body_html: String::new(),
            reviewers: Vec::new(),
            check_runs: Vec::new(),
            events: Vec::new(),
            viewer_can_review: true,
            viewer_can_comment: true,
            changed_files: Some(3),
        }
    }

    fn check(name: &str, state: CheckRunState) -> GhCheck {
        GhCheck {
            name: name.into(),
            state,
            url: None,
        }
    }

    #[test]
    fn the_excerpt_is_the_first_paragraph_as_plain_text() {
        let html = "<p>Reads the <b>relay</b> position &amp; more.</p><p>Second.</p>";
        assert_eq!(
            excerpt(html).as_deref(),
            Some("Reads the relay position & more.")
        );
        assert_eq!(
            excerpt("<p> </p><p>Then this</p>").as_deref(),
            Some("Then this")
        );
        assert_eq!(excerpt(""), None);
        let long = format!("<p>{}</p>", "word ".repeat(100));
        let cut = excerpt(&long).unwrap_or_default();
        assert!(cut.ends_with("word…"), "{cut}");
        assert!(cut.chars().count() <= 281);
    }

    #[test]
    fn the_facts_name_the_author_branch_files_and_age() {
        let detail = detail();
        let facts = facts(&detail.item, Some(&detail), i64::MAX / 2);
        let labels: Vec<&str> = facts.iter().map(|fact| fact.label).collect();
        assert_eq!(&labels[..4], ["Author", "Branch", "Files", "Updated"]);
        assert_eq!(facts[0].value, "Ann Perkins");
        assert_eq!(facts[1].value, "feature/421 → main");
        assert!(facts[1].mono);
        assert_eq!(facts[2].value, "3 files · +159 −4");
        assert!(!labels.contains(&"Assignees"), "only when there are any");
        let before = super::facts(&detail.item, None, i64::MAX / 2);
        assert_eq!(before[2].value, "+159 −4");
    }

    #[test]
    fn reviewers_map_to_a_glyph_and_a_word() {
        let mut detail = detail();
        let person = |login: &str| GhUser {
            login: login.into(),
            name: None,
        };
        detail.reviewers = vec![
            GhReviewer {
                user: person("approver"),
                state: ReviewerState::Approved,
            },
            GhReviewer {
                user: person("blocker"),
                state: ReviewerState::ChangesRequested,
            },
            GhReviewer {
                user: person("waiting"),
                state: ReviewerState::Pending,
            },
        ];
        let rows = reviewer_rows(&detail);
        let seen: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.glyph.text.as_ref(), row.word))
            .collect();
        assert_eq!(
            seen,
            [
                ("approver", "✓", "approved"),
                ("blocker", "±", "requested changes"),
                ("waiting", "•", "review requested"),
            ]
        );
    }

    #[test]
    fn failed_and_running_checks_differ_without_colour() {
        let rows = check_rows(
            &[
                check("build", CheckRunState::Success),
                check("lint", CheckRunState::Failure),
                check("e2e", CheckRunState::Pending),
            ],
            false,
        );
        let failed = &rows[1];
        let running = &rows[2];
        assert_eq!((failed.glyph.text.as_ref(), failed.word), ("✕", "failed"));
        assert_eq!(
            (running.glyph.text.as_ref(), running.word),
            ("◌", "pending")
        );
        assert_ne!(failed.glyph.text, running.glyph.text);
        assert_ne!(rows[0].glyph.text, failed.glyph.text);
    }

    #[test]
    fn above_five_checks_only_the_failing_show_until_all_are_asked_for() {
        let mut checks: Vec<GhCheck> = (0..6)
            .map(|index| check(&format!("ok-{index}"), CheckRunState::Success))
            .collect();
        checks.push(check("broken", CheckRunState::Failure));
        let collapsed = check_rows(&checks, false);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].name, "broken");
        assert_eq!(check_rows(&checks, true).len(), 7);
    }

    #[test]
    fn the_footer_offers_what_the_account_may_do() {
        let all = Allowed {
            review: true,
            comment: true,
            files: true,
            close: true,
        };
        use ItemAction::{Approve, Assign, Close, Comment, Files, RequestChanges};
        let every = [Approve, RequestChanges, Comment, Assign, Files, Close];
        assert_eq!(footer_actions(all), every);
        let read_only = Allowed {
            review: false,
            comment: false,
            files: true,
            close: false,
        };
        assert_eq!(footer_actions(read_only), [Assign, Files]);
        let merged = Allowed {
            review: false,
            close: false,
            ..all
        };
        assert_eq!(footer_actions(merged), [Comment, Assign, Files]);
        assert_eq!(review_actions(all), [Approve, RequestChanges, Comment]);
        assert_eq!(look(Approve).intent, Intent::Primary);
        let others = [RequestChanges, Comment, Assign, Files, Close];
        assert!(
            others
                .iter()
                .all(|action| look(*action).intent == Intent::Neutral)
        );
        assert_eq!(look(Close).name, "Close pull request…");
        assert_eq!(look(Assign).id, "gh-assign");
    }

    #[test]
    fn the_allowed_actions_follow_the_details_and_the_state() {
        let mut detail = detail();
        detail.viewer_can_review = false;
        detail.viewer_can_comment = false;
        let allowed = Allowed::of(&detail.item, Some(&detail));
        assert!(!allowed.review && !allowed.comment);
        assert!(allowed.files);
        assert!(allowed.close, "an open pull request can be closed");
        let before = Allowed::of(&detail.item, None);
        assert!(before.comment, "comment until the details say otherwise");
    }
}
