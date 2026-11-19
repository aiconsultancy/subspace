//! Pure placement decisions for the toolbar's responsive two-band layout.
//!
//! `Dashboard::render_toolbar` stays `Option<AnyElement>`, so the board
//! group's own natural width cannot be read through it. The shell measures
//! the group and the first row directly from GPUI layout (see
//! `subspace_dashboard::measure`) and composes the placement decision from
//! those plain widths, not a global backchannel.

/// The gap the shell leaves between the dashboard tabs and the board group
/// when both sit on the first row.
pub(super) const GROUP_GAP: f32 = 10.;

/// Whether the board's control group fits on the first row, beside the
/// dashboard tabs, instead of moving below as its own row.
///
/// Width zero means "not measured yet"; the group stays below until a real
/// measurement confirms it fits, so the first frame never overlaps the tabs.
pub(super) fn group_fits_beside_tabs(available: f32, tabs_width: f32, group_width: f32) -> bool {
    available > subspace_dashboard::measure::UNMEASURED
        && group_width > subspace_dashboard::measure::UNMEASURED
        && tabs_width + GROUP_GAP + group_width <= available
}

#[cfg(test)]
mod tests {
    use super::group_fits_beside_tabs;

    #[test]
    fn unmeasured_group_stays_below_until_a_real_width_arrives() {
        assert!(!group_fits_beside_tabs(500., 100., 0.));
    }

    #[test]
    fn unmeasured_row_stays_below_until_a_real_width_arrives() {
        assert!(!group_fits_beside_tabs(0., 100., 80.));
    }

    #[test]
    fn group_moves_beside_tabs_when_both_fit() {
        assert!(group_fits_beside_tabs(400., 100., 80.));
    }

    #[test]
    fn group_moves_below_when_it_cannot_fit_as_a_unit() {
        assert!(!group_fits_beside_tabs(150., 100., 80.));
    }

    #[test]
    fn the_gap_counts_toward_the_fit_decision() {
        assert!(!group_fits_beside_tabs(189., 100., 80.));
        assert!(group_fits_beside_tabs(190., 100., 80.));
    }
}
