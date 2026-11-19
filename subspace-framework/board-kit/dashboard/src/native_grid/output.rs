use super::{GridSnapshot, model::Model};

/// Selected includes selected records hidden by filters. View is filtered and
/// sorted; All is the original source order. Every command captures one snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridScope {
    Selected,
    View,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridFormat {
    Csv,
    Json,
}

impl Model {
    pub fn capture(&self, scope: GridScope) -> GridSnapshot {
        let rows = match scope {
            GridScope::All => (0..self.source.rows.len()).collect(),
            GridScope::View => self.shown.clone(),
            GridScope::Selected => self
                .ordered()
                .into_iter()
                .filter(|row| self.selection.cells.iter().any(|(ix, _)| ix == row))
                .collect(),
        };
        let columns: Vec<_> = (0..self.source.columns.len())
            .filter(|column| {
                scope != GridScope::Selected
                    || self.selection.cells.iter().any(|(_, ix)| ix == column)
            })
            .collect();
        GridSnapshot {
            columns: columns
                .iter()
                .map(|ix| self.source.columns[*ix].clone())
                .collect(),
            rows: rows
                .iter()
                .map(|row| {
                    columns
                        .iter()
                        .map(|column| self.source.rows[*row][*column].clone())
                        .collect()
                })
                .collect(),
        }
    }
}

impl GridSnapshot {
    /// TSV includes headers for View/All; selected-cell commands omit headers.
    pub fn tsv(&self, headers: bool) -> String {
        self.delimited('\t', headers)
    }

    pub fn encode(&self, format: GridFormat) -> Result<String, serde_json::Error> {
        match format {
            GridFormat::Csv => Ok(self.delimited(',', true)),
            GridFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
                "columns": self.columns.as_ref(),
                "rows": self.rows.iter().map(|row| row.iter().map(super::GridCell::json).collect::<Vec<_>>()).collect::<Vec<_>>()
            })),
        }
    }

    fn delimited(&self, delimiter: char, headers: bool) -> String {
        let mut rows = Vec::new();
        if headers {
            rows.push(
                self.columns
                    .iter()
                    .map(|label| quote(&safe_text(label), delimiter))
                    .collect::<Vec<_>>()
                    .join(&delimiter.to_string()),
            );
        }
        rows.extend(self.rows.iter().map(|row| {
            row.iter()
                .map(|cell| quote(&cell.spreadsheet(), delimiter))
                .collect::<Vec<_>>()
                .join(&delimiter.to_string())
        }));
        rows.join("\n")
    }
}

pub(super) fn safe_text(value: &str) -> String {
    let first = value
        .trim_start_matches(|c: char| c.is_whitespace() || c.is_control() || c == '\u{feff}')
        .chars()
        .next();
    if matches!(first, Some('=' | '+' | '-' | '@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

fn quote(value: &str, delimiter: char) -> String {
    if value.contains([delimiter, '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
