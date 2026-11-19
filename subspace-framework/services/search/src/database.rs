use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Document, Policy, Scope, files, now, storage_error};

pub(crate) struct Store {
    pub connection: Connection,
    pub path: PathBuf,
    pub revision: std::sync::Arc<crate::revision::Revision>,
}

pub(crate) fn open(path: PathBuf) -> Result<Store, String> {
    let mut connection = Connection::open(&path).map_err(storage_error)?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(storage_error)?;
    crate::migration::migrate(&mut connection)?;
    let policy = serde_json::to_string(&Policy::default()).map_err(storage_error)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO settings(id,policy,active,generation) VALUES(1,?1,NULL,0)",
            [policy],
        )
        .map_err(storage_error)?;
    let revision = std::sync::Arc::new(crate::revision::Revision::open(&path)?);
    recover_revision(&mut connection, &revision)?;
    Ok(Store {
        connection,
        path,
        revision,
    })
}

fn recover_revision(
    connection: &mut Connection,
    revision: &std::sync::Arc<crate::revision::Revision>,
) -> Result<(), String> {
    if !revision.busy() {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let _mutation = crate::revision::Mutation::begin(revision);
    transaction.commit().map_err(storage_error)
}

pub(crate) const SCHEMA: &str = "
PRAGMA journal_mode=DELETE;
PRAGMA temp_store=MEMORY;
PRAGMA secure_delete=ON;
PRAGMA synchronous=FULL;
PRAGMA cache_size=-2048;
PRAGMA auto_vacuum=INCREMENTAL;
CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1), policy TEXT NOT NULL, active TEXT, generation INTEGER NOT NULL, chosen INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS authority(id INTEGER PRIMARY KEY CHECK(id=1), epoch INTEGER NOT NULL, sequence INTEGER NOT NULL);
INSERT OR IGNORE INTO authority VALUES(1,0,0);
CREATE TABLE IF NOT EXISTS document_versions(scope TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(scope,id));
CREATE TABLE IF NOT EXISTS documents(rowid INTEGER PRIMARY KEY, scope TEXT NOT NULL, id TEXT NOT NULL, refreshed INTEGER NOT NULL, json TEXT NOT NULL, kind TEXT NOT NULL DEFAULT '', account TEXT NOT NULL DEFAULT '', project TEXT, parent_id TEXT, refreshed_at INTEGER NOT NULL DEFAULT 0, UNIQUE(scope,id));
CREATE INDEX IF NOT EXISTS documents_expiry ON documents(refreshed);
CREATE INDEX IF NOT EXISTS documents_scope_expiry ON documents(scope,refreshed);
CREATE VIRTUAL TABLE IF NOT EXISTS text_index USING fts5(title,body,metadata, tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER IF NOT EXISTS delete_document AFTER DELETE ON documents BEGIN DELETE FROM text_index WHERE rowid=old.rowid; END;
CREATE TRIGGER IF NOT EXISTS delete_version AFTER DELETE ON documents BEGIN DELETE FROM document_versions WHERE scope=old.scope AND id=old.id; END;
";

pub(crate) const FACET_SCHEMA: &str = "
CREATE INDEX IF NOT EXISTS documents_kind ON documents(kind);
CREATE INDEX IF NOT EXISTS documents_account ON documents(account);
CREATE INDEX IF NOT EXISTS documents_project ON documents(project);
CREATE INDEX IF NOT EXISTS documents_parent ON documents(parent_id);
CREATE INDEX IF NOT EXISTS documents_refreshed_at ON documents(refreshed_at);
CREATE TABLE IF NOT EXISTS \"references\"(scope TEXT NOT NULL,document_id TEXT NOT NULL,identity TEXT NOT NULL,relation_type TEXT NOT NULL,exact INTEGER NOT NULL,evidence TEXT,confidence REAL,PRIMARY KEY(scope,document_id,identity,relation_type));
CREATE INDEX IF NOT EXISTS references_identity ON \"references\"(identity);
CREATE INDEX IF NOT EXISTS references_scope_identity ON \"references\"(scope,identity);
CREATE TRIGGER IF NOT EXISTS delete_references AFTER DELETE ON documents BEGIN DELETE FROM \"references\" WHERE scope=old.scope AND document_id=old.id; END;
";

pub(crate) fn policy(connection: &Connection) -> Result<Policy, String> {
    let json: String = connection
        .query_row("SELECT policy FROM settings WHERE id=1", [], |row| {
            row.get(0)
        })
        .map_err(storage_error)?;
    serde_json::from_str(&json).map_err(storage_error)
}

pub(crate) fn selected(connection: &Connection) -> Result<(Option<String>, i64), String> {
    connection
        .query_row(
            "SELECT active,generation FROM settings WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(storage_error)
}

pub(crate) fn key(scope: &Scope) -> Result<String, String> {
    serde_json::to_string(scope).map_err(storage_error)
}

pub(crate) fn eligible(connection: &Connection, scope: &Scope) -> Result<Option<Policy>, String> {
    let policy = policy(connection)?;
    let (active, _) = selected(connection)?;
    Ok((policy.enabled && active.as_deref() == Some(&key(scope)?)).then_some(policy))
}

pub(crate) fn cutoff(policy: &Policy) -> i64 {
    const SECONDS_PER_DAY: i64 = 86_400;
    now().saturating_sub(i64::from(policy.retention_days) * SECONDS_PER_DAY)
}

pub(crate) fn read(
    connection: &rusqlite::Transaction<'_>,
    scope: &Scope,
    id: &str,
) -> Result<Option<Document>, String> {
    let Some(policy) = eligible(connection, scope)? else {
        return Ok(None);
    };
    let json: Option<String> = connection.query_row(
        "SELECT json FROM documents WHERE scope=?1 AND id=?2 AND refreshed>=?3 AND refreshed<=?4",
        params![key(scope)?, id, cutoff(&policy), now()], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    json.map(|json| serde_json::from_str(&json).map_err(storage_error))
        .transpose()
}

pub(crate) struct Pages {
    pub count: u64,
    pub size: u64,
}

impl Pages {
    pub fn bytes(&self) -> u64 {
        self.count.saturating_mul(self.size)
    }
}

pub(crate) fn pages(connection: &Connection) -> Result<Pages, String> {
    let count: u32 = connection
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(storage_error)?;
    let size: u32 = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(storage_error)?;
    Ok(Pages {
        count: u64::from(count),
        size: u64::from(size),
    })
}

pub(crate) fn usage(store: &Store) -> Result<u64, String> {
    Ok(files::usage(&store.path)?.max(pages(&store.connection)?.bytes()))
}
