//! User-selected input height, bounded by the room available to the board.

/// Kit's ordinary Input uses a 1.25 rem line height.
pub(super) const LINE_REM: f32 = 1.25;
pub(super) const TOOLBAR_REM: f32 = 2.0;
pub(super) const TOOLBAR_PADDING_PX: f32 = 8.0;
const MAX_HEIGHT_SHARE: f32 = 0.5;
const TOP_BORDER_PX: f32 = 1.0;
const PADDING_EDGES: f32 = 2.0;
const COUNTER_ROW_PADDING_REM: f32 = 0.5;

pub(super) fn chrome_height(rem: f32, pill: Option<f32>) -> f32 {
    let counter = pill.map_or(0.0, |height| height + rem * COUNTER_ROW_PADDING_REM);
    rem * TOOLBAR_REM + TOOLBAR_PADDING_PX * PADDING_EDGES + TOP_BORDER_PX + counter
}

/// Whole rows make both the native viewport and its enclosing frame agree.
pub(super) fn row_limits(
    requested: Option<f32>,
    available: f32,
    rem: f32,
    pill: Option<f32>,
    padding: f32,
) -> (usize, usize) {
    // A manually sized editor keeps its rows when the counter appears.
    // Its counter overlays the frame; only automatic growth reserves a row.
    let chrome = chrome_height(rem, pill.filter(|_| requested.is_none()));
    let line = rem * LINE_REM;
    let maximum = ((available * MAX_HEIGHT_SHARE - chrome - padding) / line)
        .floor()
        .max(1.0) as usize;
    match requested {
        Some(height) => {
            let rows = ((height - chrome - padding) / line)
                .floor()
                .max(super::MIN_ROWS as f32) as usize;
            (rows.min(maximum), rows.min(maximum))
        }
        None => (super::MIN_ROWS.min(maximum), super::MAX_ROWS.min(maximum)),
    }
}

#[cfg(test)]
mod tests {
    use super::row_limits;

    #[test]
    fn resizing_keeps_the_shared_minimum_and_never_exceeds_half_the_board() {
        assert_eq!(row_limits(None, 900.0, 16.0, None, 16.0), (2, 12));
        assert_eq!(row_limits(None, 400.0, 16.0, None, 16.0), (2, 6));
        assert_eq!(row_limits(Some(20.0), 900.0, 16.0, None, 16.0), (2, 2));
        assert_eq!(row_limits(Some(1_000.0), 900.0, 16.0, None, 16.0), (19, 19));
        assert_eq!(row_limits(Some(400.0), 600.0, 16.0, None, 16.0), (11, 11));
        assert_eq!(row_limits(None, 100.0, 16.0, None, 16.0), (1, 1));
    }

    #[test]
    fn counter_reserves_space_only_for_automatic_growth() {
        let pill = Some(17.75);
        assert_eq!(row_limits(None, 400.0, 16.0, pill, 16.0), (2, 5));
        for available in [100.0, 400.0, 900.0] {
            for requested in [Some(180.0), Some(1_000.0)] {
                assert_eq!(
                    row_limits(requested, available, 16.0, pill, 16.0),
                    row_limits(requested, available, 16.0, None, 16.0),
                    "the counter must not take a manually selected row"
                );
            }
        }
    }
}
