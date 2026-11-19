//! Rail geometry and drag rules shared by render, resize, and persistence.

use subspace_dashboard::badge;

pub const BUTTON: f32 = 32.;
pub const INSET: f32 = 4.;
pub const BORDER: f32 = 1.;
pub const ICON: f32 = 18.;
pub const GAP: f32 = 8.;
pub const BADGE_PAD: f32 = badge::PADDING_X_PX;
pub const BADGE_FONT: f32 = badge::FONT_SIZE_PX;
const SIDES: f32 = 2.;

pub fn icon_pad() -> f32 {
    (BUTTON - ICON) / SIDES
}

pub fn collapsed_width() -> f32 {
    INSET + BUTTON + INSET
}

pub fn inner_end_inset() -> f32 {
    INSET - BORDER
}

pub fn badge_box(text_width: f32) -> f32 {
    BADGE_PAD * SIDES + text_width
}

pub fn entry_width(has_icon: bool, label: f32, badge: Option<f32>) -> f32 {
    let mut inner = icon_pad() * SIDES + label;
    if has_icon {
        inner += ICON + GAP;
    }
    if let Some(badge) = badge {
        inner += GAP + badge;
    }
    INSET + inner + INSET
}

pub fn content_max(entries: impl IntoIterator<Item = f32>) -> f32 {
    entries.into_iter().fold(collapsed_width(), f32::max)
}

pub fn displayed_width(
    collapsed: bool,
    preferred: Option<f32>,
    content_max: f32,
    collapsed_width: f32,
) -> f32 {
    if collapsed {
        return collapsed_width;
    }
    let cap = content_max.max(collapsed_width);
    preferred.unwrap_or(cap).clamp(collapsed_width, cap)
}

pub struct DragOutcome {
    pub collapsed: bool,
    pub preferred: Option<f32>,
}

pub fn drag_to(size: f32, collapsed_width: f32, content_max: f32) -> DragOutcome {
    let cap = content_max.max(collapsed_width);
    let size = size.clamp(collapsed_width, cap);
    if size <= collapsed_width {
        DragOutcome {
            collapsed: true,
            preferred: None,
        }
    } else {
        DragOutcome {
            collapsed: false,
            preferred: Some(size),
        }
    }
}

pub fn read_width(value: Option<&str>) -> Option<f32> {
    value
        .and_then(|value| value.parse().ok())
        .filter(|width: &f32| width.is_finite() && *width > collapsed_width())
}

#[cfg(test)]
mod tests {
    use super::{collapsed_width, content_max, displayed_width, drag_to, entry_width, read_width};

    #[test]
    fn a_temporary_content_cap_does_not_replace_the_preferred_width() {
        let preferred = Some(180.);
        let collapsed = collapsed_width();
        assert_eq!(
            displayed_width(false, preferred, 90., collapsed),
            90.,
            "display clamps to the live cap"
        );
        assert_eq!(preferred, Some(180.), "the stored preference is unchanged");
        assert_eq!(
            displayed_width(false, preferred, 200., collapsed),
            180.,
            "the preference returns when the cap lifts"
        );
    }

    #[test]
    fn dragging_to_the_collapsed_edge_keeps_the_preferred_expanded_width() {
        let collapsed = collapsed_width();
        let outcome = drag_to(collapsed, collapsed, 160.);
        assert!(outcome.collapsed);
        assert_eq!(outcome.preferred, None);
    }

    #[test]
    fn dragging_out_from_icons_records_the_live_width_as_preferred() {
        let collapsed = collapsed_width();
        let outcome = drag_to(128., collapsed, 160.);
        assert!(!outcome.collapsed);
        assert_eq!(outcome.preferred, Some(128.));
    }

    #[test]
    fn a_drag_beyond_the_content_cap_is_stored_at_the_cap() {
        let collapsed = collapsed_width();
        let outcome = drag_to(400., collapsed, 150.);
        assert!(!outcome.collapsed);
        assert_eq!(outcome.preferred, Some(150.));
    }

    #[test]
    fn invalid_stored_widths_are_ignored() {
        assert_eq!(read_width(None), None);
        assert_eq!(read_width(Some("nope")), None);
        assert_eq!(read_width(Some("-8")), None);
        assert_eq!(read_width(Some("1")), None);
        assert_eq!(read_width(Some("40")), None);
        assert_eq!(read_width(Some("nan")), None);
        assert_eq!(read_width(Some("140.5")), Some(140.5));
    }

    #[test]
    fn content_max_is_the_widest_entry_not_a_fixed_rail() {
        let collapsed = collapsed_width();
        let short = entry_width(true, 20., None);
        let wide = entry_width(true, 80., Some(18.));
        assert!(wide > short);
        assert_eq!(content_max([short, wide]), wide);
        assert_eq!(content_max(Vec::<f32>::new()), collapsed);
        assert_ne!(
            content_max([short, wide]),
            192.,
            "a fixed 192 px rail must not win over measured entries"
        );
    }

    #[test]
    fn collapsed_display_ignores_a_stored_expanded_preference() {
        let collapsed = collapsed_width();
        assert_eq!(
            displayed_width(true, Some(180.), 200., collapsed),
            collapsed
        );
    }
}
