pub(crate) use crate::stores::{require_eligible, store, store_mut};
use crate::stores::{validate_name, validate_policy};
use crate::{
    Document, Lease, Policy, Reranker, RerankerStatus, Scope, SourceStatus, Target,
    database::{self, Store},
    files, limits, maintenance, selection, storage_error, writes,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, RwLock},
};

#[derive(Clone)]
pub struct Service {
    pub(crate) inner: Arc<Inner>,
}

/// A board's rule for documents no search may show at the moment.
pub(crate) type Hidden = Arc<dyn Fn(&Document) -> bool + Send + Sync>;

pub(crate) struct Inner {
    app: String,
    pub stores: Mutex<BTreeMap<String, Store>>,
    pub reranker: RwLock<Option<Arc<dyn Reranker>>>,
    revisions: BTreeMap<String, Arc<crate::revision::Revision>>,
    pub(crate) hidden: RwLock<BTreeMap<String, Hidden>>,
}

impl Service {
    pub fn search(&self, query: &crate::Query) -> Result<crate::SearchResponse, String> {
        let reranker = self.inner.reranker.read().map_err(storage_error)?.clone();
        crate::query::search(self, query, reranker)
    }

    pub fn open(root: PathBuf, app: String, boards: Vec<String>) -> Result<Self, String> {
        validate_name(&app)?;
        if boards.len() > limits::MAX_BOARDS {
            return Err("Too many search boards".into());
        }
        let root = files::directory(&root, &app)?
            .canonicalize()
            .map_err(storage_error)?;
        let mut stores = BTreeMap::new();
        for board in boards {
            validate_name(&board)?;
            let path = files::database_path(&root, &board)?;
            stores.insert(board, database::open(path)?);
        }
        let revisions = stores
            .iter()
            .map(|(board, store)| (board.clone(), store.revision.clone()))
            .collect();
        Ok(Self {
            inner: Arc::new(Inner {
                app,
                stores: Mutex::new(stores),
                reranker: RwLock::new(None),
                revisions,
                hidden: RwLock::new(BTreeMap::new()),
            }),
        })
    }

    pub fn app_id(&self) -> &str {
        &self.inner.app
    }

    pub(crate) fn stores(&self) -> Result<MutexGuard<'_, BTreeMap<String, Store>>, String> {
        self.inner.stores.lock().map_err(storage_error)
    }

    pub fn policy(&self, board: &str) -> Result<Policy, String> {
        let stores = self.stores()?;
        database::policy(&store(&stores, board)?.connection)
    }

    /// The person's choice of `board`'s policy, which a board's default never
    /// replaces afterwards.
    pub fn set_policy(&self, board: &str, policy: Policy) -> Result<(), String> {
        self.revoke(board)?;
        validate_policy(&policy)?;
        let mut stores = self.stores()?;
        maintenance::set_policy(store_mut(&mut stores, board)?, &policy, true)
    }

    /// Applies the policy a board declares as its default, until the person
    /// chooses one with [`Service::set_policy`]. True when it changed the
    /// policy.
    pub fn offer_default(&self, board: &str, policy: Policy) -> Result<bool, String> {
        validate_policy(&policy)?;
        let mut stores = self.stores()?;
        let changed = maintenance::offer_default(store_mut(&mut stores, board)?, &policy)?;
        if changed {
            self.revoke(board)?;
        }
        Ok(changed)
    }

    pub fn activate(&self, scope: &Scope) -> Result<(), String> {
        let activation = self.prepare_activation(scope)?;
        self.activate_prepared(&activation)
    }

    /// Background only: durable ticket capture before a routine identity lookup.
    pub fn begin_activation(&self, board: &str) -> Result<crate::ActivationTicket, String> {
        let mut stores = self.stores()?;
        selection::begin(store_mut(&mut stores, board)?, false)
    }

    /// Background only: explicitly detach the old account before a switch lookup.
    // qual:api
    pub fn begin_transition(&self, board: &str) -> Result<crate::ActivationTicket, String> {
        self.revoke(board)?;
        let mut stores = self.stores()?;
        selection::begin(store_mut(&mut stores, board)?, true)
    }

    pub fn prepare_activation(&self, scope: &Scope) -> Result<crate::Activation, String> {
        self.validate_scope(scope)?;
        let ticket = self.begin_activation(&scope.board)?;
        self.prepare_activation_ticket(&ticket, scope)
    }

    pub fn prepare_activation_ticket(
        &self,
        ticket: &crate::ActivationTicket,
        scope: &Scope,
    ) -> Result<crate::Activation, String> {
        self.validate_scope(scope)?;
        let stores = self.stores()?;
        selection::prepare(store(&stores, &scope.board)?, ticket, scope)
    }

    pub fn activate_prepared(&self, activation: &crate::Activation) -> Result<(), String> {
        self.validate_scope(&activation.scope)?;
        let mut stores = self.stores()?;
        selection::activate(store_mut(&mut stores, &activation.scope.board)?, activation)
    }

    pub fn lease(&self, scope: &Scope) -> Result<Lease, String> {
        self.validate_scope(scope)?;
        let mut stores = self.stores()?;
        writes::lease(store_mut(&mut stores, &scope.board)?, scope)
    }

    pub fn upsert(&self, scope: &Scope, documents: &[Document]) -> Result<(), String> {
        let lease = self.lease(scope)?;
        self.upsert_leased(&lease, documents)
    }

    /// Captures a fresh write sequence only while the original run's generation
    /// remains active. Clear/re-enable and away/back transitions cannot revive it.
    pub fn renew_lease(&self, lease: &Lease) -> Result<Lease, String> {
        self.validate_scope(&lease.scope)?;
        let mut stores = self.stores()?;
        writes::lease_generation(
            store_mut(&mut stores, &lease.scope.board)?,
            &lease.scope,
            Some(lease.generation),
        )
    }

    pub fn upsert_leased(&self, lease: &Lease, documents: &[Document]) -> Result<(), String> {
        self.validate_scope(&lease.scope)?;
        let mut stores = self.stores()?;
        writes::upsert(
            store_mut(&mut stores, &lease.scope.board)?,
            lease,
            documents,
        )
    }

    /// Replaces relation identities and the optional project facet for one
    /// retained document. Board adapters call this after a successful publish.
    // qual:api
    pub fn index_metadata(
        &self,
        scope: &Scope,
        id: &str,
        metadata: &crate::IndexMetadata,
    ) -> Result<(), String> {
        self.index_metadata_batch(scope, &[(id.to_owned(), metadata.clone())])
    }

    /// [`Service::index_metadata`] for up to 100 retained documents in one
    /// transaction.
    pub fn index_metadata_batch(
        &self,
        scope: &Scope,
        entries: &[(String, crate::IndexMetadata)],
    ) -> Result<(), String> {
        self.validate_scope(scope)?;
        let mut stores = self.stores()?;
        crate::metadata::write(store_mut(&mut stores, &scope.board)?, scope, entries)
    }

    /// A board's own retained documents matching the query's filters, newest
    /// first, with their payloads. Fails when the scope is not active or
    /// caching is off.
    pub fn records(
        &self,
        scope: &Scope,
        query: &crate::RecordQuery,
    ) -> Result<crate::RecordPage, String> {
        self.validate_scope(scope)?;
        crate::records::page(store(&*self.stores()?, &scope.board)?, scope, query)
    }

    /// The parents of a board's retained documents matching `filters`, with
    /// their counts, newest first.
    pub fn groups(
        &self,
        scope: &Scope,
        filters: &crate::Filters,
        limit: usize,
    ) -> Result<Vec<crate::RecordGroup>, String> {
        self.validate_scope(scope)?;
        crate::records::groups(
            store(&*self.stores()?, &scope.board)?,
            scope,
            filters,
            limit,
        )
    }

    /// Deletes every retained document of the scope matching `filters`, which
    /// must name at least one facet. Returns how many went.
    // qual:api
    pub fn remove_matching(
        &self,
        scope: &Scope,
        filters: &crate::Filters,
    ) -> Result<usize, String> {
        let lease = self.lease(scope)?;
        let mut stores = self.stores()?;
        crate::records::remove(store_mut(&mut stores, &scope.board)?, &lease, filters)
    }

    pub fn search_filtered(
        &self,
        query: &crate::Query,
        filters: &crate::Filters,
    ) -> Result<crate::SearchResponse, String> {
        let reranker = self.inner.reranker.read().map_err(storage_error)?.clone();
        crate::query::search_filtered(self, query, filters, reranker)
    }

    pub fn related_items(
        &self,
        reference: &crate::ReferenceIdentity,
        query: &crate::RelatedQuery,
    ) -> Result<Vec<crate::RelatedItem>, String> {
        let stores = self.inner.stores.lock().map_err(storage_error)?;
        crate::metadata::related(&stores, reference, query)
    }

    pub fn remove(&self, scope: &Scope, ids: &[String]) -> Result<(), String> {
        let lease = self.lease(scope)?;
        self.remove_leased(&lease, ids)
    }

    pub fn remove_leased(&self, lease: &Lease, ids: &[String]) -> Result<(), String> {
        self.validate_scope(&lease.scope)?;
        let mut stores = self.stores()?;
        writes::remove(store_mut(&mut stores, &lease.scope.board)?, lease, ids)
    }

    pub fn clear(&self, board: &str) -> Result<(), String> {
        self.revoke(board)?;
        let mut stores = self.stores()?;
        maintenance::clear(store_mut(&mut stores, board)?)
    }

    pub fn sweep(&self) -> Result<(), String> {
        let mut stores = self.stores()?;
        for store in stores.values_mut() {
            maintenance::sweep(store)?;
        }
        Ok(())
    }

    pub fn invalidate(&self, scope: &Scope) -> Result<(), String> {
        self.revoke(&scope.board)?;
        self.validate_scope(scope)?;
        let mut stores = self.stores()?;
        writes::invalidate(store_mut(&mut stores, &scope.board)?, scope)
    }

    pub fn read(&self, target: &Target) -> Result<Option<Document>, String> {
        self.validate_scope(&target.scope)?;
        if target.id.len() > limits::MAX_IDENTIFIER {
            return Err("Search target exceeds size limit".into());
        }
        let stores = self.stores()?;
        let connection = &store(&stores, &target.scope.board)?.connection;
        let snapshot = connection.unchecked_transaction().map_err(storage_error)?;
        database::read(&snapshot, &target.scope, &target.id)
    }

    pub fn read_fenced(&self, target: &Target) -> Result<Option<crate::CachedRead>, String> {
        self.validate_scope(&target.scope)?;
        if target.id.len() > limits::MAX_IDENTIFIER {
            return Err("Search target exceeds size limit".into());
        }
        let stores = self.stores()?;
        crate::fenced_read::read(store(&stores, &target.scope.board)?, target)
    }

    pub(crate) fn revoke(&self, board: &str) -> Result<(), String> {
        self.inner
            .revisions
            .get(board)
            .ok_or("Board does not participate in search")?
            .revoke();
        Ok(())
    }

    pub fn sources(&self) -> Result<Vec<SourceStatus>, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        self.stores()?
            .iter_mut()
            .map(|(board, store)| crate::sources::source(board, store, deadline))
            .collect()
    }

    pub fn set_reranker(&self, reranker: Arc<dyn Reranker>) {
        if let Ok(mut slot) = self.inner.reranker.write() {
            *slot = Some(reranker);
        }
    }

    pub fn reranker_status(&self) -> Option<RerankerStatus> {
        let reranker = self.inner.reranker.read().ok()?.clone();
        reranker.map(|reranker| reranker.status())
    }

    pub fn install_reranker(&self) -> Result<(), String> {
        let reranker = self.inner.reranker.read().map_err(storage_error)?.clone();
        reranker
            .ok_or("Local reranker is unavailable")?
            .install()
            .map_err(|_| "Local reranker installation failed".into())
    }

    pub(crate) fn validate_scope(&self, scope: &Scope) -> Result<(), String> {
        if scope.app != self.app_id()
            || scope.account.is_empty()
            || scope.endpoint.is_empty()
            || [&scope.account, &scope.endpoint, &scope.tenant]
                .iter()
                .any(|value| value.len() > limits::MAX_IDENTIFIER)
        {
            return Err("Invalid search scope".into());
        }
        Ok(())
    }
}
