//! The board-facing paths beside search: a board reads its own retained
//! documents back by facet, newest first, lists their parents, and deletes
//! them by facet. Every call is held to the active scope, the policy and the
//! retention window, as search is.

use rusqlite::{Connection, TransactionBehavior, params_from_iter, types::Value};

use crate::{
    Document, Filters, Lease, Policy, RecordCursor, RecordGroup, RecordPage, RecordQuery, Scope,
    database::{self, Store},
    limits,
    query::{push_facets, validate_filters},
    service::require_eligible,
    storage_error,
};

pub(crate) fn page(
    store: &Store,
    scope: &Scope,
    query: &RecordQuery,
) -> Result<RecordPage, String> {
    validate_query(query)?;
    let snapshot = store
        .connection
        .unchecked_transaction()
        .map_err(storage_error)?;
    let policy = require_eligible(&snapshot, scope)?;
    let (sql, values) = page_sql(&database::key(scope)?, &policy, query);
    let documents = documents(&snapshot, &sql, values)?;
    Ok(split_page(documents, query.limit))
}

pub(crate) fn groups(
    store: &Store,
    scope: &Scope,
    filters: &Filters,
    limit: usize,
) -> Result<Vec<RecordGroup>, String> {
    validate_filters(filters)?;
    validate_limit(limit)?;
    let snapshot = store
        .connection
        .unchecked_transaction()
        .map_err(storage_error)?;
    let policy = require_eligible(&snapshot, scope)?;
    let (sql, values) = group_sql(&database::key(scope)?, &policy, filters, limit);
    read_groups(&snapshot, &sql, values)
}

pub(crate) fn remove(store: &mut Store, lease: &Lease, filters: &Filters) -> Result<usize, String> {
    validate_removal(filters)?;
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    require_eligible(&transaction, &lease.scope)?;
    if database::selected(&transaction)?.1 != lease.generation {
        return Err("Search removal was cancelled".into());
    }
    let (sql, values) = removal_sql(&database::key(&lease.scope)?, filters);
    let removed = transaction
        .execute(&sql, params_from_iter(values))
        .map_err(storage_error)?;
    if removed > 0 {
        transaction
            .execute("UPDATE settings SET generation=generation+1", [])
            .map_err(storage_error)?;
    }
    transaction.commit().map_err(storage_error)?;
    Ok(removed)
}

/// The scope, retention window and facets every path shares.
fn base(scope: &str, policy: &Policy, filters: &Filters) -> (String, Vec<Value>) {
    let mut sql =
        " FROM documents d WHERE d.scope=? AND d.refreshed>=? AND d.refreshed<=?".to_owned();
    let mut values: Vec<Value> = vec![
        scope.to_owned().into(),
        database::cutoff(policy).into(),
        crate::now().into(),
    ];
    push_facets(&mut sql, &mut values, filters);
    (sql, values)
}

fn page_sql(scope: &str, policy: &Policy, query: &RecordQuery) -> (String, Vec<Value>) {
    let (from, mut values) = base(scope, policy, &query.filters);
    let mut sql = format!("SELECT d.json{from}");
    if let Some(cursor) = &query.before {
        sql.push_str(" AND (d.refreshed_at<? OR (d.refreshed_at=? AND d.id<?))");
        values.extend([
            cursor.refreshed_at.into(),
            cursor.refreshed_at.into(),
            cursor.id.clone().into(),
        ]);
    }
    sql.push_str(" ORDER BY d.refreshed_at DESC,d.id DESC LIMIT ?");
    values.push(sql_limit(query.limit.saturating_add(1)));
    (sql, values)
}

fn group_sql(
    scope: &str,
    policy: &Policy,
    filters: &Filters,
    limit: usize,
) -> (String, Vec<Value>) {
    let (from, mut values) = base(scope, policy, filters);
    let sql = format!(
        "SELECT d.parent_id,count(*),max(d.refreshed_at){from} AND d.parent_id IS NOT NULL GROUP BY d.parent_id ORDER BY max(d.refreshed_at) DESC,d.parent_id LIMIT ?"
    );
    values.push(sql_limit(limit));
    (sql, values)
}

/// Removal ignores the retention window: expired records go too.
fn removal_sql(scope: &str, filters: &Filters) -> (String, Vec<Value>) {
    let mut sql =
        "DELETE FROM documents WHERE rowid IN (SELECT d.rowid FROM documents d WHERE d.scope=?"
            .to_owned();
    let mut values: Vec<Value> = vec![scope.to_owned().into()];
    push_facets(&mut sql, &mut values, filters);
    sql.push(')');
    (sql, values)
}

fn sql_limit(limit: usize) -> Value {
    Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX))
}

fn documents(
    connection: &Connection,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<Document>, String> {
    let mut statement = connection.prepare(sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    rows.map(|row| serde_json::from_str(&row.map_err(storage_error)?).map_err(storage_error))
        .collect()
}

fn read_groups(
    connection: &Connection,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<RecordGroup>, String> {
    let mut statement = connection.prepare(sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok(RecordGroup {
                parent_id: row.get(0)?,
                records: row.get::<_, u32>(1)?.into(),
                newest: row.get(2)?,
            })
        })
        .map_err(storage_error)?;
    rows.map(|row| row.map_err(storage_error)).collect()
}

/// The first `limit` documents, and the cursor after them when there are
/// more.
fn split_page(mut documents: Vec<Document>, limit: usize) -> RecordPage {
    let more = documents.len() > limit;
    documents.truncate(limit);
    let next = documents.last().filter(|_| more).map(|last| RecordCursor {
        refreshed_at: last.refreshed_at,
        id: last.id.clone(),
    });
    RecordPage { documents, next }
}

fn validate_query(query: &RecordQuery) -> Result<(), String> {
    validate_filters(&query.filters)?;
    validate_limit(query.limit)?;
    if query
        .before
        .as_ref()
        .is_some_and(|cursor| cursor.id.len() > limits::MAX_IDENTIFIER)
    {
        return Err("Search record cursor exceeds its bounds".into());
    }
    Ok(())
}

fn validate_limit(limit: usize) -> Result<(), String> {
    if !(1..=limits::MAX_RECORDS).contains(&limit) {
        return Err("Search record page exceeds its bounds".into());
    }
    Ok(())
}

/// A removal names at least one facet; [`crate::Service::clear`] removes
/// everything.
fn validate_removal(filters: &Filters) -> Result<(), String> {
    validate_filters(filters)?;
    if *filters == Filters::default() {
        return Err("Search removal needs a filter".into());
    }
    Ok(())
}
