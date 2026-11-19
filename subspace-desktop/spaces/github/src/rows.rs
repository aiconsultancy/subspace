//! Rows and group headers: state icon, `#number`, title, then review, CI, diff stat and author for
//! pull requests, or label dots, comments and assignee for issues, then the
//! age.

use subspace_framework::client_github::model::{
    CheckRunState, CheckState, GhItem, GhLabel, GhUser, ItemKind, PrState, ReviewDecision,
    ReviewerState,
};
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use crate::model::{age, group_by_repo, group_key, same_login, user_initials};

/// An empty slot keeps the trailing glyphs of rows aligned.
fn slot() -> Glyph {
    Glyph::new(" ", Tone::Muted)
}

/// State icon colours from `styles.css` (`.gh-pr-open`, `.gh-issue-closed`, …).
pub fn state_glyph(item: &GhItem) -> Glyph {
    match (item.kind, item.state) {
        (ItemKind::Issue, PrState::Closed | PrState::Merged) => Glyph::new("✓", Tone::Magenta),
        (ItemKind::Issue, _) => Glyph::new("◉", Tone::Green),
        (ItemKind::Pr, PrState::Open) => Glyph::new("⇅", Tone::Green),
        (ItemKind::Pr, PrState::Draft) => Glyph::new("⇅", Tone::Muted),
        (ItemKind::Pr, PrState::Merged) => Glyph::new("⑂", Tone::Magenta),
        (ItemKind::Pr, PrState::Closed) => Glyph::new("⇅", Tone::Red),
    }
}

pub fn state_label(item: &GhItem) -> &'static str {
    match (item.kind, item.state) {
        (_, PrState::Open) => "Open",
        (_, PrState::Draft) => "Draft",
        (ItemKind::Pr, PrState::Merged) => "Merged",
        (ItemKind::Issue, PrState::Merged) | (_, PrState::Closed) => "Closed",
    }
}

/// `.gh-review-approved`, `.gh-review-changes_requested`, …
pub fn review_glyph(decision: Option<ReviewDecision>) -> Glyph {
    match decision {
        Some(ReviewDecision::Approved) => Glyph::new("✓", Tone::Green),
        Some(ReviewDecision::ChangesRequested) => Glyph::new("±", Tone::Red),
        Some(ReviewDecision::ReviewRequired) => Glyph::new("•", Tone::Muted),
        None => slot(),
    }
}

/// `.gh-ci-success`, `.gh-ci-failure`, `.gh-ci-pending`.
pub fn ci_glyph(checks: Option<CheckState>) -> Glyph {
    match checks {
        Some(CheckState::Success) => Glyph::new("✓", Tone::Green),
        Some(CheckState::Failure) => Glyph::new("✕", Tone::Red),
        Some(CheckState::Pending) => Glyph::new("◌", Tone::Yellow),
        None => slot(),
    }
}

/// A reviewer's glyph and `REVIEW_TAG` word in the panel (`ReviewerGlyph`).
pub fn reviewer_badge(state: ReviewerState) -> (Glyph, &'static str) {
    match state {
        ReviewerState::Approved => (Glyph::new("✓", Tone::Green), "approved"),
        ReviewerState::ChangesRequested => (Glyph::new("±", Tone::Red), "requested changes"),
        ReviewerState::Commented => (Glyph::new("▭", Tone::Muted), "reviewed"),
        ReviewerState::Pending => (Glyph::new("•", Tone::Yellow), "review requested"),
        ReviewerState::Dismissed => (Glyph::new("○", Tone::Muted), "dismissed"),
    }
}

/// A check's glyph and state word in the panel.
pub fn check_glyph(state: CheckRunState) -> (Glyph, &'static str) {
    match state {
        CheckRunState::Success => (Glyph::new("✓", Tone::Green), "passed"),
        CheckRunState::Failure => (Glyph::new("✕", Tone::Red), "failed"),
        CheckRunState::Pending => (Glyph::new("◌", Tone::Yellow), "pending"),
        CheckRunState::Neutral => (Glyph::new("○", Tone::Muted), "neutral"),
        CheckRunState::Skipped => (Glyph::new("⊘", Tone::Muted), "skipped"),
        CheckRunState::Cancelled => (Glyph::new("⊘", Tone::Red), "cancelled"),
    }
}

const TONES: [Tone; 7] = [
    Tone::Blue,
    Tone::Magenta,
    Tone::Cyan,
    Tone::Orange,
    Tone::Green,
    Tone::Red,
    Tone::Yellow,
];

/// The multiplier of the login hash that picks a tone.
const LOGIN_HASH_PRIME: u32 = 31;

/// A stable tone per login; the account's own badge is the accent.
pub fn person_tone(login: &str, account: &str) -> Tone {
    if same_login(login, account) {
        return Tone::Accent;
    }
    let hash = login.to_lowercase().bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(LOGIN_HASH_PRIME)
            .wrapping_add(u32::from(byte))
    });
    TONES[hash as usize % TONES.len()]
}

pub fn person_glyph(user: Option<&GhUser>, account: &str) -> Glyph {
    match user {
        Some(user) => Glyph::new(user_initials(user), person_tone(&user.login, account)),
        None => Glyph::new("··", Tone::Muted),
    }
}

const HEX_RADIX: u32 = 16;
const HEX_DIGITS: usize = 6;
const CHANNEL_MASK: u32 = 0xff;
const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;

/// The red, green and blue channels of a six-digit hex colour.
fn rgb_channels(hex: &str) -> Option<[f32; 3]> {
    if hex.len() != HEX_DIGITS {
        return None;
    }
    let value = u32::from_str_radix(hex, HEX_RADIX).ok()?;
    Some(
        [
            (value >> RED_SHIFT) & CHANNEL_MASK,
            (value >> GREEN_SHIFT) & CHANNEL_MASK,
            value & CHANNEL_MASK,
        ]
        .map(|channel| channel as f32),
    )
}

/// Degrees per sector of the hue wheel, and the sector each primary starts in.
const SECTOR_DEGREES: f32 = 60.0;
const SECTOR_COUNT: f32 = 6.0;
const GREEN_SECTOR: f32 = 2.0;
const BLUE_SECTOR: f32 = 4.0;
/// Colours with less chroma than this read as grey.
const GREY_CHROMA_LIMIT: f32 = 24.0;

/// The hue of a colour in degrees, or `None` when it is too grey to place.
fn hue_degrees([r, g, b]: [f32; 3]) -> Option<f32> {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    if chroma < GREY_CHROMA_LIMIT {
        return None;
    }
    if max == r {
        Some(SECTOR_DEGREES * ((g - b) / chroma).rem_euclid(SECTOR_COUNT))
    } else if max == g {
        Some(SECTOR_DEGREES * ((b - r) / chroma + GREEN_SECTOR))
    } else {
        Some(SECTOR_DEGREES * ((r - g) / chroma + BLUE_SECTOR))
    }
}

/// The upper hue bound in degrees of each tone, in order.
const HUE_SECTORS: [(u32, Tone); 6] = [
    (14, Tone::Red),
    (39, Tone::Orange),
    (69, Tone::Yellow),
    (159, Tone::Green),
    (199, Tone::Cyan),
    (259, Tone::Blue),
];
/// Hues from here to the top of the wheel are red again.
const RED_WRAP_DEGREES: u32 = 330;

/// The tone of a hue in degrees.
fn tone_for_hue(hue: f32) -> Tone {
    let degrees = hue as u32;
    if degrees >= RED_WRAP_DEGREES {
        return Tone::Red;
    }
    HUE_SECTORS
        .iter()
        .find(|(limit, _)| degrees <= *limit)
        .map_or(Tone::Magenta, |(_, tone)| *tone)
}

/// The tone nearest a label's hex colour: the shared glyphs carry tones,
/// not free colours.
pub fn label_tone(label: &GhLabel) -> Tone {
    match rgb_channels(label.color.trim_start_matches('#')).and_then(hue_degrees) {
        Some(hue) => tone_for_hue(hue),
        None => Tone::Muted,
    }
}

fn padded(glyph: Glyph, width: usize) -> Glyph {
    Glyph::new(format!("{:<width$}", glyph.text), glyph.tone)
}

/// At most this many label dots show; the rest become `+n`.
const MAX_LABEL_DOTS: usize = 3;
/// The width of the `+n` slot that replaces the hidden dots.
const OVERFLOW_WIDTH: usize = 3;

/// Up to three label dots and `+n` (`LabelDots`), in a fixed width.
fn label_dots(labels: &[GhLabel]) -> Vec<Glyph> {
    let mut dots: Vec<Glyph> = labels
        .iter()
        .take(MAX_LABEL_DOTS)
        .map(|label| Glyph::new("●", label_tone(label)))
        .collect();
    let slots = MAX_LABEL_DOTS.saturating_sub(dots.len());
    if labels.len() > MAX_LABEL_DOTS {
        dots.push(Glyph::new(
            format!("+{:<2}", labels.len() - MAX_LABEL_DOTS),
            Tone::Muted,
        ));
    } else {
        dots.insert(
            0,
            Glyph::new(" ".repeat(slots * 2 + OVERFLOW_WIDTH), Tone::Muted),
        );
    }
    dots
}

pub fn row(item: &GhItem, account: &str, now: i64) -> Row {
    let mut trailing = match item.kind {
        ItemKind::Pr => {
            let mut glyphs = vec![review_glyph(item.review_decision), ci_glyph(item.checks)];
            // The UI font is monospaced: padding keeps the columns aligned.
            let (additions, deletions) = match (item.additions, item.deletions) {
                (Some(additions), Some(deletions)) => {
                    (format!("+{additions}"), format!("−{deletions}"))
                }
                _ => (String::new(), String::new()),
            };
            glyphs.push(Glyph::new(format!("{additions:>6}"), Tone::Green));
            glyphs.push(Glyph::new(format!("{deletions:<5}"), Tone::Red));
            glyphs.push(padded(person_glyph(item.author.as_ref(), account), 2));
            glyphs
        }
        ItemKind::Issue => {
            let mut glyphs = label_dots(&item.labels);
            let comments = if item.comments > 0 {
                format!("▭ {}", item.comments)
            } else {
                String::new()
            };
            glyphs.push(Glyph::new(format!("{comments:>5}"), Tone::Muted));
            glyphs.push(padded(person_glyph(item.assignees.first(), account), 2));
            glyphs
        }
    };
    let age = age(&item.updated_at, now);
    trailing.push(Glyph::new(format!("{age:>3}"), Tone::Muted));
    Row {
        key: item.key.clone().into(),
        icon: state_glyph(item),
        id: Some(format!("#{}", item.number).into()),
        title: item.title.clone().into(),
        trailing,
        done: item.state == PrState::Draft,
        indent: 0,
    }
}

/// The groups of one section's items.
pub fn groups(items: &[GhItem], account: &str, now: i64) -> Vec<Group> {
    group_by_repo(items)
        .into_iter()
        .map(|group| {
            let quiet_owner = group
                .repo
                .split_once('/')
                .is_some_and(|(owner, _)| same_login(owner, account));
            Group {
                key: group_key(group.repo).into(),
                icon: Glyph::new("▣", if quiet_owner { Tone::Muted } else { Tone::Blue }),
                name: group.repo.to_string().into(),
                count: Some(group.items.len().to_string().into()),
                rows: group
                    .items
                    .iter()
                    .map(|item| row(item, account, now))
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::{
        CheckRunState, GhLabel, ItemKind, ReviewerState,
    };
    use subspace_framework::dashboard::Tone;

    use super::{check_glyph, groups, label_tone, person_tone, reviewer_badge};
    use crate::sample;

    #[test]
    fn reviewer_states_have_their_panel_glyph_and_tag() {
        let expected = [
            (ReviewerState::Approved, "✓", Tone::Green, "approved"),
            (
                ReviewerState::ChangesRequested,
                "±",
                Tone::Red,
                "requested changes",
            ),
            (ReviewerState::Commented, "▭", Tone::Muted, "reviewed"),
            (
                ReviewerState::Pending,
                "•",
                Tone::Yellow,
                "review requested",
            ),
            (ReviewerState::Dismissed, "○", Tone::Muted, "dismissed"),
        ];

        for (state, text, tone, tag) in expected {
            let (glyph, actual_tag) = reviewer_badge(state);
            assert_eq!(glyph.text.as_ref(), text);
            assert_eq!(glyph.tone, tone);
            assert_eq!(actual_tag, tag);
        }
    }

    #[test]
    fn check_states_have_their_panel_glyph_and_word() {
        let expected = [
            (CheckRunState::Success, "✓", Tone::Green, "passed"),
            (CheckRunState::Failure, "✕", Tone::Red, "failed"),
            (CheckRunState::Pending, "◌", Tone::Yellow, "pending"),
            (CheckRunState::Neutral, "○", Tone::Muted, "neutral"),
            (CheckRunState::Skipped, "⊘", Tone::Muted, "skipped"),
            (CheckRunState::Cancelled, "⊘", Tone::Red, "cancelled"),
        ];

        for (state, text, tone, word) in expected {
            let (glyph, actual_word) = check_glyph(state);
            assert_eq!(glyph.text.as_ref(), text);
            assert_eq!(glyph.tone, tone);
            assert_eq!(actual_word, word);
        }
    }

    #[test]
    fn rows_show_number_glyphs_and_age() {
        let items: Vec<_> = sample::items()
            .into_iter()
            .filter(|item| item.kind == ItemKind::Pr)
            .collect();
        let groups = groups(&items, "someone", i64::MAX / 2);
        let row = &groups[0].rows[0];
        assert_eq!(row.id.as_deref(), Some("#421"));
        let texts: Vec<&str> = row.trailing.iter().map(|g| g.text.as_ref()).collect();
        assert!(
            texts.iter().any(|t| t.trim_start().starts_with('+')),
            "{texts:?}"
        );
        assert!(texts.last().is_some_and(|age| age.ends_with('y')));
    }

    #[test]
    fn label_colours_map_to_the_nearest_tone() {
        let label = |color: &str| GhLabel {
            name: "x".into(),
            color: color.into(),
        };
        assert_eq!(label_tone(&label("d73a4a")), Tone::Red);
        assert_eq!(label_tone(&label("0075ca")), Tone::Blue);
        assert_eq!(label_tone(&label("a2eeef")), Tone::Cyan);
        assert_eq!(label_tone(&label("ededed")), Tone::Muted);
        assert_eq!(label_tone(&label("nothex")), Tone::Muted);
        assert_eq!(person_tone("Me", "me"), Tone::Accent);
    }
}
