//! The grid a session runs on (REQ-17 groundwork).

use portable_pty::PtySize;

/// The grid a session runs on, in cells and in pixels per cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionSize {
    /// Columns of text.
    pub columns: u16,
    /// Rows of text.
    pub rows: u16,
    /// The width of one cell in pixels.
    pub cell_width: u16,
    /// The height of one cell in pixels.
    pub cell_height: u16,
}

impl SessionSize {
    /// A grid of at least one row and one column.
    pub(crate) fn clamped(self) -> Self {
        Self {
            columns: self.columns.max(1),
            rows: self.rows.max(1),
            ..self
        }
    }

    /// The window size the kernel reports to the program. `ws_xpixel` and
    /// `ws_ypixel` are the whole grid, not one cell.
    pub(crate) fn pty_size(self) -> PtySize {
        PtySize {
            rows: self.rows,
            cols: self.columns,
            pixel_width: self.columns.saturating_mul(self.cell_width),
            pixel_height: self.rows.saturating_mul(self.cell_height),
        }
    }
}

impl Default for SessionSize {
    fn default() -> Self {
        Self {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::grid::SessionSize;

    #[test]
    fn a_grid_keeps_at_least_one_row_and_column() {
        let size = SessionSize {
            columns: 0,
            rows: 0,
            cell_width: 7,
            cell_height: 15,
        }
        .clamped();

        assert_eq!(
            size,
            SessionSize {
                columns: 1,
                rows: 1,
                cell_width: 7,
                cell_height: 15,
            }
        );
    }

    #[test]
    fn the_window_size_reports_the_whole_grid_in_pixels() {
        let size = SessionSize {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        }
        .pty_size();

        assert_eq!((size.pixel_width, size.pixel_height), (640, 384));
    }
}
