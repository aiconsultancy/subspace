use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub row: usize,
    pub column: usize,
}

#[derive(Default)]
pub(super) struct Selection {
    pub anchor: Option<Cell>,
    pub active: Option<Cell>,
    pub cells: BTreeSet<(usize, usize)>,
}

impl Selection {
    pub fn select(&mut self, cell: Cell, extend: bool, shown: &[usize]) {
        let anchor = self
            .anchor
            .filter(|anchor| extend && shown.contains(&anchor.row))
            .unwrap_or(cell);
        let start = shown.iter().position(|row| *row == anchor.row).unwrap_or(0);
        let end = shown
            .iter()
            .position(|row| *row == cell.row)
            .unwrap_or(start);
        self.cells = shown[start.min(end)..=start.max(end)]
            .iter()
            .flat_map(|row| {
                (anchor.column.min(cell.column)..=anchor.column.max(cell.column))
                    .map(move |column| (*row, column))
            })
            .collect();
        self.anchor = Some(anchor);
        self.active = Some(cell);
    }

    pub fn destination(&self, key: &str, shown: &[usize], columns: usize) -> Option<Cell> {
        if shown.is_empty() || columns == 0 {
            return None;
        }
        let active = self.active.unwrap_or(Cell {
            row: shown[0],
            column: 0,
        });
        let row = shown.iter().position(|row| *row == active.row).unwrap_or(0);
        let column = active.column.min(columns - 1);
        let (row, column) = match key {
            "up" => (row.saturating_sub(1), column),
            "down" => ((row + 1).min(shown.len() - 1), column),
            "left" => (row, column.saturating_sub(1)),
            "right" => (row, (column + 1).min(columns - 1)),
            "home" => (row, 0),
            "end" => (row, columns - 1),
            _ => return None,
        };
        Some(Cell {
            row: shown[row],
            column,
        })
    }
}
