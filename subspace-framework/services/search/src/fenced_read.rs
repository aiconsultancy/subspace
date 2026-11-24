//! Reading one record by its citation, plain or fenced for cached
//! navigation. A record its board's rule hides reads as absent.

use crate::{
    CachedRead, Document, ReadPermit, Service, Target,
    database::{self, Store},
    hidden::hides,
    limits,
    service::store,
    storage_error,
};

// qual:allow(coupling, oi) reason: "Reading by citation is one concern of the service, kept apart from its stores and writes in service.rs."
impl Service {
    pub fn read(&self, target: &Target) -> Result<Option<Document>, String> {
        self.validate_scope(&target.scope)?;
        if target.id.len() > limits::MAX_IDENTIFIER {
            return Err("Search target exceeds size limit".into());
        }
        let rules = self.rules()?;
        let stores = self.stores()?;
        let connection = &store(&stores, &target.scope.board)?.connection;
        let snapshot = connection.unchecked_transaction().map_err(storage_error)?;
        Ok(database::read(&snapshot, &target.scope, &target.id)?
            .filter(|document| !hides(&rules, &target.scope.board, document)))
    }

    pub fn read_fenced(&self, target: &Target) -> Result<Option<CachedRead>, String> {
        self.validate_scope(&target.scope)?;
        if target.id.len() > limits::MAX_IDENTIFIER {
            return Err("Search target exceeds size limit".into());
        }
        let rules = self.rules()?;
        let stores = self.stores()?;
        Ok(read(store(&stores, &target.scope.board)?, target)?
            .filter(|read| !hides(&rules, &target.scope.board, &read.document)))
    }
}

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
