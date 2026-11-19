use super::{GridSnapshot, selection::Selection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sort {
    Ascending(usize),
    Descending(usize),
}

pub(super) struct Model {
    pub source: GridSnapshot,
    pub shown: Vec<usize>,
    pub filters: Vec<String>,
    pub sort: Option<Sort>,
    pub selection: Selection,
}

impl Model {
    pub fn new(source: GridSnapshot) -> Self {
        let count = source.columns.len();
        let shown = (0..source.rows.len()).collect();
        Self {
            source,
            shown,
            filters: vec![String::new(); count],
            sort: None,
            selection: Selection::default(),
        }
    }

    pub fn replace(&mut self, source: GridSnapshot) {
        let columns_changed = self.source.columns != source.columns;
        self.source = source;
        if columns_changed {
            self.filters = vec![String::new(); self.source.columns.len()];
            self.sort = None;
        }
        self.selection = Selection::default();
        self.rebuild();
    }

    pub fn cycle_sort(&mut self, column: usize) {
        self.sort = match self.sort {
            Some(Sort::Ascending(ix)) if ix == column => Some(Sort::Descending(column)),
            Some(Sort::Descending(ix)) if ix == column => None,
            _ => Some(Sort::Ascending(column)),
        };
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        let filters: Vec<_> = self
            .filters
            .iter()
            .map(|value| value.to_lowercase())
            .collect();
        self.shown = self
            .ordered()
            .into_iter()
            .filter(|row| {
                self.source.rows[*row]
                    .iter()
                    .zip(&filters)
                    .all(|(cell, filter)| {
                        filter.is_empty() || cell.display().to_lowercase().contains(filter)
                    })
            })
            .collect();
    }

    pub fn ordered(&self) -> Vec<usize> {
        let mut rows: Vec<_> = (0..self.source.rows.len()).collect();
        if let Some(sort) = self.sort {
            let (column, descending) = match sort {
                Sort::Ascending(column) => (column, false),
                Sort::Descending(column) => (column, true),
            };
            rows.sort_by(|left, right| {
                let order =
                    self.source.rows[*left][column].compare(&self.source.rows[*right][column]);
                if descending { order.reverse() } else { order }
            });
        }
        rows
    }
}
