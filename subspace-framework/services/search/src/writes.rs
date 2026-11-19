use rusqlite::{TransactionBehavior, params};

const EVICTION_STEPS: [u32; 12] = [0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1000];

use crate::{
    Document, Lease, Scope, admission,
    database::{self, Store},
    limits,
    service::require_eligible,
    storage_error,
};

pub(crate) fn upsert(
    store: &mut Store,
    lease: &Lease,
    documents: &[Document],
) -> Result<(), String> {
    let encoded = admission::encode(documents)?;
    for evict in EVICTION_STEPS {
        if attempt(store, lease, documents, &encoded, evict)? {
            return Ok(());
        }
    }
    Err("Search quota cannot admit this batch within the eviction limit".into())
}

fn attempt(
    store: &mut Store,
    lease: &Lease,
    documents: &[Document],
    encoded: &[String],
    evict: u32,
) -> Result<bool, String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let policy = require_eligible(&transaction, &lease.scope)?;
    if database::selected(&transaction)?.1 != lease.generation {
        return Err("Search write was cancelled".into());
    }
    admission::reserve(&transaction, &store.path, &policy)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    let scope = database::key(&lease.scope)?;
    crate::maintenance::evict_oldest(&transaction, evict, &scope, documents)?;
    for (document, json) in documents.iter().zip(encoded) {
        match insert(&transaction, &scope, document, json, lease.sequence) {
            Ok(()) => {}
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DiskFull =>
            {
                return Ok(false);
            }
            Err(error) => return Err(storage_error(error)),
        }
    }
    match transaction.commit() {
        Ok(()) => Ok(true),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::DiskFull =>
        {
            Ok(false)
        }
        Err(error) => Err(storage_error(error)),
    }
}

fn insert(
    connection: &rusqlite::Connection,
    scope: &str,
    document: &Document,
    json: &str,
    sequence: i64,
) -> rusqlite::Result<()> {
    let account: String = serde_json::from_str::<Scope>(scope)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
        .account;
    let changed = connection.execute("INSERT INTO documents(scope,id,refreshed,json,kind,account,parent_id,refreshed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?3) ON CONFLICT(scope,id) DO UPDATE SET refreshed=excluded.refreshed,json=excluded.json,kind=excluded.kind,account=excluded.account,parent_id=excluded.parent_id,refreshed_at=excluded.refreshed_at WHERE excluded.refreshed>=documents.refreshed AND ?8>=COALESCE((SELECT revision FROM document_versions WHERE scope=?1 AND id=?2),0)", params![scope, document.id, document.refreshed_at, json, document.kind, account, document.parent_id, sequence])?;
    if changed == 0 {
        return Ok(());
    }
    connection.execute("INSERT INTO document_versions VALUES(?1,?2,?3) ON CONFLICT(scope,id) DO UPDATE SET revision=excluded.revision", params![scope, document.id, sequence])?;
    let rowid: i64 = connection.query_row(
        "SELECT rowid FROM documents WHERE scope=?1 AND id=?2",
        params![scope, document.id],
        |row| row.get(0),
    )?;
    connection.execute("DELETE FROM text_index WHERE rowid=?1", [rowid])?;
    connection.execute(
        "INSERT INTO text_index(rowid,title,body,metadata) VALUES(?1,?2,?3,?4)",
        params![rowid, document.title, document.body, document.metadata],
    )?;
    Ok(())
}

pub(crate) fn remove(store: &mut Store, lease: &Lease, ids: &[String]) -> Result<(), String> {
    if ids.len() > limits::MAX_RESULTS || ids.iter().any(|id| id.len() > limits::MAX_IDENTIFIER) {
        return Err("Search removal exceeds batch limit".into());
    }
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    require_eligible(&transaction, &lease.scope)?;
    if database::selected(&transaction)?.1 != lease.generation {
        return Err("Search removal was cancelled".into());
    }
    let scope = database::key(&lease.scope)?;
    for id in ids {
        transaction
            .execute(
                "DELETE FROM documents WHERE scope=?1 AND id=?2 AND COALESCE((SELECT revision FROM document_versions WHERE scope=?1 AND id=?2),0)<=?3",
                params![scope, id, lease.sequence],
            )
            .map_err(storage_error)?;
    }
    if !ids.is_empty() {
        transaction
            .execute("UPDATE settings SET generation=generation+1", [])
            .map_err(storage_error)?;
    }
    transaction.commit().map_err(storage_error)
}

pub(crate) fn lease(store: &mut Store, scope: &Scope) -> Result<Lease, String> {
    lease_generation(store, scope, None)
}

pub(crate) fn lease_generation(
    store: &mut Store,
    scope: &Scope,
    expected: Option<i64>,
) -> Result<Lease, String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    require_eligible(&transaction, scope)?;
    let generation = database::selected(&transaction)?.1;
    if expected.is_some_and(|expected| expected != generation) {
        return Err("Search prefetch generation changed".into());
    }
    transaction
        .execute("UPDATE authority SET sequence=sequence+1", [])
        .map_err(storage_error)?;
    let sequence = transaction
        .query_row("SELECT sequence FROM authority", [], |row| row.get(0))
        .map_err(storage_error)?;
    transaction.commit().map_err(storage_error)?;
    Ok(Lease {
        scope: scope.clone(),
        generation,
        sequence,
    })
}

pub(crate) fn invalidate(store: &mut Store, scope: &Scope) -> Result<(), String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(&store.revision);
    let changed = transaction.execute("UPDATE settings SET active=NULL,generation=generation+1 WHERE active=?1 OR active IS NULL", [database::key(scope)?]).map_err(storage_error)?;
    if changed != 0 {
        transaction
            .execute("UPDATE authority SET epoch=epoch+1", [])
            .map_err(storage_error)?;
    }
    transaction.commit().map_err(storage_error)
}
