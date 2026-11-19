use rusqlite::TransactionBehavior;

use crate::{
    Policy,
    database::{self, Store},
    storage_error,
};

/// Whether the person has chosen this store's policy.
fn chosen(store: &Store) -> Result<bool, String> {
    store
        .connection
        .query_row("SELECT chosen FROM settings WHERE id=1", [], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(storage_error)
}

/// Applies a board's default `policy` until the person chose one. True when
/// it changed the policy.
pub(crate) fn offer_default(store: &mut Store, policy: &Policy) -> Result<bool, String> {
    if chosen(store)? || database::policy(&store.connection)? == *policy {
        return Ok(false);
    }
    set_policy(store, policy, false)?;
    Ok(true)
}

pub(crate) fn set_policy(store: &mut Store, policy: &Policy, chosen: bool) -> Result<(), String> {
    let json = serde_json::to_string(policy).map_err(storage_error)?;
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    if policy.enabled {
        crate::admission::reserve(&transaction, &store.path, policy)?;
    }
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    if !policy.enabled {
        erase(&transaction)?;
    }
    transaction
        .execute(
            "UPDATE settings SET policy=?1,chosen=MAX(chosen,?2)",
            rusqlite::params![json, chosen],
        )
        .map_err(storage_error)?;
    transaction.commit().map_err(storage_error)?;
    if !policy.enabled {
        store
            .connection
            .execute_batch("VACUUM")
            .map_err(storage_error)?;
    }
    Ok(())
}

fn erase(connection: &rusqlite::Connection) -> Result<(), String> {
    connection
        .execute("DELETE FROM documents", [])
        .map_err(storage_error)?;
    connection
        .execute("INSERT INTO text_index(text_index) VALUES('rebuild')", [])
        .map_err(storage_error)?;
    connection
        .execute(
            "UPDATE settings SET active=NULL,generation=generation+1",
            [],
        )
        .map_err(storage_error)?;
    connection
        .execute("UPDATE authority SET epoch=epoch+1", [])
        .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn clear(store: &mut Store) -> Result<(), String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    erase(&transaction)?;
    transaction.commit().map_err(storage_error)?;
    store
        .connection
        .execute_batch("VACUUM")
        .map_err(storage_error)
}

pub(crate) fn sweep(store: &mut Store) -> Result<(), String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    let policy = database::policy(&transaction)?;
    transaction.execute("DELETE FROM documents WHERE rowid IN (SELECT rowid FROM documents WHERE refreshed<?1 LIMIT 1000)", [database::cutoff(&policy)]).map_err(storage_error)?;
    transaction.commit().map_err(storage_error)?;
    store
        .connection
        .execute_batch("PRAGMA incremental_vacuum(128)")
        .map_err(storage_error)
}

pub(crate) fn evict_oldest(
    connection: &rusqlite::Connection,
    count: u32,
    scope: &str,
    documents: &[crate::Document],
) -> Result<(), String> {
    let ids: Vec<&str> = documents
        .iter()
        .map(|document| document.id.as_str())
        .collect();
    let ids = serde_json::to_string(&ids).map_err(storage_error)?;
    connection.execute(
        "DELETE FROM documents WHERE rowid IN (SELECT rowid FROM documents WHERE NOT (scope=?2 AND id IN (SELECT value FROM json_each(?3))) ORDER BY refreshed,rowid LIMIT ?1)",
        rusqlite::params![count, scope, ids],
    ).map_err(storage_error)?;
    if count > 0 {
        connection
            .execute("INSERT INTO text_index(text_index) VALUES('optimize')", [])
            .map_err(storage_error)?;
    }
    Ok(())
}
