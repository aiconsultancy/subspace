//! A pull request's tile under the message that keeps it: its status glyph,
//! tinted as the inline reference's, and `#n`, the title, then its status when not open, the repository and
//! the lines added and removed, and its checks as a small bar with
//! `passed/total`. Only what the answering Space gives is shown, and a
//! missing fact leaves no gap.

use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, div,
};
use subspace_framework::contracts::github::{CheckStatus, PullRequestDetails, PullRequestStatus};
use subspace_framework::dashboard::{Tokens, Tone};

use super::tile::{changes, status};
use crate::tiles::{self, Lead, Slot};

/// What separates the facts on the second line.
const SEPARATOR: &str = " · ";

/// `#n` of canonical `owner/repo#n`.
pub(super) fn number(key: &str) -> String {
    key.rsplit_once('#')
        .map_or_else(|| key.to_owned(), |(_, number)| format!("#{number}"))
}

/// `owner/repo` of canonical `owner/repo#n`.
pub(super) fn repository(key: &str) -> &str {
    key.rsplit_once('#').map_or(key, |(repo, _)| repo)
}

/// The status in words when it is not simply open.
pub(super) fn status_words(details: &PullRequestDetails) -> Option<&'static str> {
    (details.status != PullRequestStatus::Open).then(|| status(details.status).2)
}

/// The checks as `passed/total`, their spoken form and the bar's tone:
/// red once a check failed, green when all passed, yellow while running.
pub(super) fn checks(details: &PullRequestDetails) -> Option<(u32, u32, String, String, Tone)> {
    let counts = details.check_counts.filter(|counts| counts.total > 0)?;
    let tone = match details.checks {
        Some(CheckStatus::Failure) => Tone::Red,
        _ if counts.passed >= counts.total => Tone::Green,
        _ => Tone::Yellow,
    };
    Some((
        counts.passed,
        counts.total,
        format!("{}/{}", counts.passed, counts.total),
        format!("{} of {} checks passed", counts.passed, counts.total),
        tone,
    ))
}

/// The tile's accessible name: what it opens and every fact it shows.
pub(super) fn label(details: &PullRequestDetails) -> String {
    let mut facts = vec![status(details.status).2.to_owned()];
    facts.push(repository(&details.key).to_owned());
    if let Some((added, removed)) = changes(details) {
        facts.push(format!("{added} {removed}"));
    }
    if let Some((_, _, _, spoken, _)) = checks(details) {
        facts.push(spoken);
    }
    format!(
        "Open pull request {}: {}, {}",
        details.key,
        details.title,
        facts.join(", ")
    )
}

/// The tile's content.
pub(super) fn tile(details: &PullRequestDetails, tokens: &Tokens, cx: &App) -> Div {
    let (glyph, _, _) = status(details.status);
    let tone = match crate::text::look::pull_request(details.status) {
        crate::text::Look::Tint(tone) => tone,
        _ => Tone::Muted,
    };
    let slot = Slot {
        lead: Lead {
            mark: Some(
                Icon::from(glyph)
                    .size(tokens.secondary_size())
                    .text_color(tokens.tone(tone))
                    .into_any_element(),
            ),
            key: number(&details.key),
        },
        title: details.title.clone().into(),
        meta: meta(details, tokens),
        trailing: meter(details, tokens),
    };
    tiles::tile(slot, tokens, cx)
}

fn meter(details: &PullRequestDetails, tokens: &Tokens) -> Option<AnyElement> {
    checks(details).map(|(passed, total, words, spoken, tone)| {
        let id = SharedString::from(format!("pr-tile-checks:{}", details.key));
        tiles::meter(id, (passed, total), (words, spoken), tone, tokens)
    })
}

/// The second line: the status when not open, the repository, then the
/// lines added in green and removed in red.
fn meta(details: &PullRequestDetails, tokens: &Tokens) -> AnyElement {
    let changes = changes(details);
    h_flex()
        .min_w_0()
        .children(status_words(details).map(|words| format!("{words}{SEPARATOR}")))
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .child(repository(&details.key).to_owned()),
        )
        .when_some(changes, |line, (added, removed)| {
            line.child(SEPARATOR).child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .child(div().text_color(tokens.green).child(added))
                    .child(div().text_color(tokens.red).child(removed)),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use subspace_framework::contracts::github::{
        CheckCounts, CheckStatus, PullRequestDetails, PullRequestStatus,
    };
    use subspace_framework::dashboard::Tone;

    use super::{checks, label, number, repository, status_words};

    fn bare(status: PullRequestStatus) -> PullRequestDetails {
        PullRequestDetails::new(
            "acme/web#492",
            "Cancel tile decodes that leave the viewport",
            "https://github.com/acme/web/pull/492",
            status,
        )
    }

    #[test]
    fn the_key_splits_into_number_and_repository() {
        assert_eq!(number("acme/web#492"), "#492");
        assert_eq!(repository("acme/web#492"), "acme/web");
    }

    #[test]
    fn a_bare_open_pull_request_says_only_what_it_has() {
        let details = bare(PullRequestStatus::Open);
        assert_eq!(status_words(&details), None);
        assert!(checks(&details).is_none());
        assert_eq!(
            label(&details),
            "Open pull request acme/web#492: Cancel tile decodes that leave the viewport, Open, acme/web"
        );
    }

    #[test]
    fn a_full_pull_request_names_changes_and_checks() {
        let details = bare(PullRequestStatus::Open)
            .with_changes(Some(214), Some(61))
            .with_checks(Some(CheckStatus::Pending))
            .with_check_counts(Some(CheckCounts::new(4, 5)));
        let (passed, total, words, spoken, tone) = checks(&details).expect("checks");
        assert_eq!((passed, total, words.as_str()), (4, 5, "4/5"));
        assert_eq!(spoken, "4 of 5 checks passed");
        assert_eq!(tone, Tone::Yellow);
        assert!(label(&details).ends_with("acme/web, +214 −61, 4 of 5 checks passed"));
    }

    #[test]
    fn checks_take_red_on_failure_and_green_when_all_passed() {
        let tone = |status, passed| {
            let details = bare(PullRequestStatus::Open)
                .with_checks(Some(status))
                .with_check_counts(Some(CheckCounts::new(passed, 5)));
            checks(&details).map(|(.., tone)| tone)
        };
        assert_eq!(tone(CheckStatus::Failure, 4), Some(Tone::Red));
        assert_eq!(tone(CheckStatus::Success, 5), Some(Tone::Green));
        let none = bare(PullRequestStatus::Open).with_check_counts(Some(CheckCounts::new(0, 0)));
        assert!(checks(&none).is_none(), "no checks, no bar");
    }

    #[test]
    fn a_status_other_than_open_is_said_in_words() {
        assert_eq!(
            status_words(&bare(PullRequestStatus::Merged)),
            Some("Merged")
        );
        assert_eq!(status_words(&bare(PullRequestStatus::Draft)), Some("Draft"));
    }
}
