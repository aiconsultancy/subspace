use rusqlite::types::Value;

use crate::{Filters, Policy, database};

const MAX_FACET_BYTES: usize = 1024;

pub(super) fn sql(
    expression: &str,
    active: &str,
    policy: &Policy,
    filters: &Filters,
) -> (String, Vec<Value>) {
    let mut sql = "SELECT d.json,snippet(text_index,1,'','',' … ',32),bm25(text_index,5.0,1.0,2.0) FROM text_index JOIN documents d ON d.rowid=text_index.rowid WHERE text_index MATCH ? AND d.scope=? AND d.refreshed>=? AND d.refreshed<=?".to_owned();
    let mut values = vec![
        expression.to_owned().into(),
        active.to_owned().into(),
        database::cutoff(policy).into(),
        crate::now().into(),
    ];
    push_facets(&mut sql, &mut values, filters);
    sql.push_str(" ORDER BY bm25(text_index,5.0,1.0,2.0),d.id");
    (sql, values)
}

/// Appends `AND` clauses for every facet `filters` sets, on the documents
/// table aliased `d`, with the values as parameters.
pub(crate) fn push_facets(sql: &mut String, values: &mut Vec<Value>, filters: &Filters) {
    push_text(sql, values, "d.kind", filters.kind.as_ref());
    push_text(sql, values, "d.account", filters.account.as_ref());
    push_text(sql, values, "d.project", filters.project.as_ref());
    push_text(sql, values, "d.parent_id", filters.parent_id.as_ref());
    push_number(sql, values, "d.refreshed_at>=?", filters.refreshed_after);
    push_number(sql, values, "d.refreshed_at<=?", filters.refreshed_before);
}

fn push_text(sql: &mut String, values: &mut Vec<Value>, column: &str, value: Option<&String>) {
    if let Some(value) = value {
        sql.push_str(" AND ");
        sql.push_str(column);
        sql.push_str("=?");
        values.push(value.clone().into());
    }
}

fn push_number(sql: &mut String, values: &mut Vec<Value>, clause: &str, value: Option<i64>) {
    if let Some(value) = value {
        sql.push_str(" AND ");
        sql.push_str(clause);
        values.push(value.into());
    }
}

pub(crate) fn validate(filters: &Filters) -> Result<(), String> {
    let values = [
        filters.kind.as_ref(),
        filters.account.as_ref(),
        filters.project.as_ref(),
        filters.parent_id.as_ref(),
    ];
    if values
        .into_iter()
        .flatten()
        .any(|value| value.is_empty() || value.len() > MAX_FACET_BYTES)
        || filters
            .refreshed_after
            .zip(filters.refreshed_before)
            .is_some_and(|(after, before)| after > before)
    {
        return Err("Search filter is invalid or exceeds its bounds".into());
    }
    Ok(())
}
