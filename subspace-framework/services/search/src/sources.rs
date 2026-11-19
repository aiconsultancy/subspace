use crate::{
    Scope, SourceStatus,
    database::{self, Store},
    storage_error,
};

pub(crate) fn source(
    board: &str,
    store: &mut Store,
    deadline: std::time::Instant,
) -> Result<SourceStatus, String> {
    let deadline = crate::deadline::Deadline::install(store, deadline)?;
    let store = deadline.store();
    let snapshot = store
        .connection
        .unchecked_transaction()
        .map_err(storage_error)?;
    let policy = database::policy(&snapshot)?;
    let (active, _) = database::selected(&snapshot)?;
    let scope: Option<Scope> = active
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(storage_error)?;
    let active = active.filter(|_| scope.is_some());
    let records = snapshot
        .query_row(
            "SELECT count(*) FROM documents WHERE scope=?1 AND refreshed>=?2 AND refreshed<=?3",
            rusqlite::params![active, database::cutoff(&policy), crate::now()],
            |row| row.get::<_, u32>(0),
        )
        .map_err(storage_error)?;
    Ok(SourceStatus {
        board: board.into(),
        enabled: policy.enabled,
        account: scope.map(|scope| scope.account),
        records: u64::from(records),
        bytes: database::usage(store)?,
        message: coverage(policy.enabled, active.is_some()).into(),
    })
}

fn coverage(enabled: bool, selected: bool) -> &'static str {
    match (enabled, selected) {
        (false, _) => "Caching disabled",
        (true, false) => "No verified account selected; waiting for an authorized fetch",
        (true, true) => {
            "Partial history: only fetched content is indexed; expired content is hidden. Usage includes database, index and sidecars; writes reserve rollback space."
        }
    }
}
