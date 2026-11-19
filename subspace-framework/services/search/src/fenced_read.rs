use crate::{
    CachedRead, ReadPermit, Target,
    database::{self, Store},
    storage_error,
};

pub(crate) fn read(store: &Store, target: &Target) -> Result<Option<CachedRead>, String> {
    if !crate::revision::Revision::SUPPORTED {
        return Err("Fenced cached navigation is unavailable on this platform".into());
    }
    let observed = store.revision.observed();
    let snapshot = store
        .connection
        .unchecked_transaction()
        .map_err(storage_error)?;
    let Some(document) = database::read(&snapshot, &target.scope, &target.id)? else {
        return Ok(None);
    };
    let policy = database::policy(&snapshot)?;
    const SECONDS_PER_DAY: i64 = 86_400;
    let expires = document
        .refreshed_at
        .saturating_add(i64::from(policy.retention_days) * SECONDS_PER_DAY);
    let permit = ReadPermit {
        revision: store.revision.clone(),
        observed,
        expires,
    };
    Ok(permit.valid().then_some(CachedRead { document, permit }))
}
