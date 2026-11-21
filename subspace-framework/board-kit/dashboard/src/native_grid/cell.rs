use std::sync::Arc;

const MAX_ROWS: usize = 500;
const MAX_CELLS: usize = 8_000;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Number(f64),
    String(String),
}

/// A typed number or string. Snapshot construction rejects non-finite numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct GridCell(Value);

impl GridCell {
    pub fn number(value: f64) -> Self {
        Self(Value::Number(value))
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self(Value::String(value.into()))
    }

    pub(super) fn display(&self) -> String {
        match &self.0 {
            Value::Number(value) => value.to_string(),
            Value::String(value) => value.clone(),
        }
    }

    pub(super) fn is_numeric(&self) -> bool {
        matches!(self.0, Value::Number(_))
    }

    pub(super) fn compare(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.0, &other.0) {
            (Value::Number(left), Value::Number(right)) => {
                left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Number(_), Value::String(_)) => std::cmp::Ordering::Less,
            (Value::String(_), Value::Number(_)) => std::cmp::Ordering::Greater,
            (Value::String(left), Value::String(right)) => left.cmp(right),
        }
    }

    pub(super) fn json(&self) -> serde_json::Value {
        match &self.0 {
            Value::Number(value) => serde_json::Value::from(*value),
            Value::String(value) => serde_json::Value::from(value.clone()),
        }
    }

    pub(super) fn spreadsheet(&self) -> String {
        match &self.0 {
            Value::Number(value) => value.to_string(),
            Value::String(value) => super::output::safe_text(value),
        }
    }

    fn is_valid(&self) -> bool {
        match &self.0 {
            Value::Number(value) => value.is_finite(),
            Value::String(_) => true,
        }
    }
}

/// Immutable result. Row identity is its original position within this snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct GridSnapshot {
    pub(super) columns: Arc<[String]>,
    pub(super) rows: Arc<[Vec<GridCell>]>,
}

impl GridSnapshot {
    /// Preserve Space Lab's existing 500-row / 8,000-cell result boundary.
    pub fn new(columns: Vec<String>, rows: Vec<Vec<GridCell>>) -> Result<Self, &'static str> {
        if columns.is_empty()
            || columns.len() > MAX_CELLS
            || rows.len() > MAX_ROWS
            || rows.len().saturating_mul(columns.len()) > MAX_CELLS
            || rows
                .iter()
                .any(|row| row.len() != columns.len() || !row.iter().all(GridCell::is_valid))
        {
            return Err("Grid requires rectangular finite data within 500 rows and 8,000 cells");
        }
        Ok(Self {
            columns: columns.into(),
            rows: rows.into(),
        })
    }
}
