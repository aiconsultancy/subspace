use crate::{
    Scope,
    database::{self, Store},
    storage_error,
};
use rusqlite::TransactionBehavior;
use std::path::PathBuf;

/// Durable compare-and-set activation, prepared on a background worker.
#[derive(Clone, Debug)]
pub struct Activation {
    pub(crate) scope: Scope,
    path: PathBuf,
    epoch: i64,
}

/// Capture on a background worker before an asynchronous identity read.
#[derive(Clone, Debug)]
pub struct ActivationTicket {
    path: PathBuf,
    epoch: i64,
}

pub(crate) fn begin(store: &mut Store, transition: bool) -> Result<ActivationTicket, String> {
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = transition.then(|| crate::revision::Mutation::begin(&store.revision));
    transaction
        .execute("UPDATE authority SET epoch=epoch+1", [])
        .map_err(storage_error)?;
    if transition {
        transaction
            .execute(
                "UPDATE settings SET active=NULL,generation=generation+1",
                [],
            )
            .map_err(storage_error)?;
    }
    let epoch = epoch(&transaction)?;
    transaction.commit().map_err(storage_error)?;
    Ok(ActivationTicket {
        path: store.path.clone(),
        epoch,
    })
}

pub(crate) fn prepare(
    store: &Store,
    ticket: &ActivationTicket,
    scope: &Scope,
) -> Result<Activation, String> {
    if ticket.path != store.path || ticket.epoch != epoch(&store.connection)? {
        return Err("Search identity observation was cancelled".into());
    }
    Ok(Activation {
        scope: scope.clone(),
        path: ticket.path.clone(),
        epoch: ticket.epoch,
    })
}

pub(crate) fn activate(store: &mut Store, activation: &Activation) -> Result<(), String> {
    if activation.path != store.path {
        return Err("Search activation belongs to another store".into());
    }
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let changed_scope =
        database::selected(&transaction)?.0.as_deref() != Some(&database::key(&activation.scope)?);
    let _mutation = changed_scope.then(|| crate::revision::Mutation::begin(&store.revision));
    let changed = transaction
        .execute(
            "UPDATE authority SET epoch=epoch+1 WHERE epoch=?1",
            [activation.epoch],
        )
        .map_err(storage_error)?;
    if changed == 0 {
        return Err("Search activation was cancelled".into());
    }
    transaction
        .execute(
            "UPDATE settings SET active=?1,generation=generation+1 WHERE active IS NOT ?1",
            [database::key(&activation.scope)?],
        )
        .map_err(storage_error)?;
    transaction.commit().map_err(storage_error)
}

fn epoch(connection: &rusqlite::Connection) -> Result<i64, String> {
    connection
        .query_row("SELECT epoch FROM authority", [], |row| row.get(0))
        .map_err(storage_error)
}
