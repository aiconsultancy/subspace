//! Resolve a preferred logical width against the current workspace and UI zoom.

const MIN_WIDTH_REM: f32 = 16.;
const MAX_WIDTH_SHARE: f32 = 0.6;
const MIN_VISIBLE_WIDTH: f32 = 1.;

pub(super) fn shown_width(preferred: f32, available: f32, rem: f32) -> f32 {
    let maximum = (available * MAX_WIDTH_SHARE).max(MIN_VISIBLE_WIDTH);
    let minimum = (rem * MIN_WIDTH_REM).min(maximum);
    preferred.clamp(minimum, maximum)
}

#[cfg(test)]
mod tests {
    use super::shown_width;

    #[test]
    fn narrowing_is_temporary_and_widening_restores_the_preference() {
        assert_eq!(shown_width(500., 600., 16.), 360.);
        assert_eq!(shown_width(500., 2_000., 16.), 500.);
    }

    #[test]
    fn small_windows_and_ui_zoom_leave_room_for_the_workspace() {
        assert_eq!(shown_width(1., 1_000., 16.), 256.);
        assert_eq!(shown_width(1., 1_000., 24.), 384.);
        assert!((shown_width(500., 200., 24.) - 120.).abs() < 0.001);
    }
}
