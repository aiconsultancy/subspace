use rusqlite::{Connection, TransactionBehavior};

use crate::{database, storage_error};

const VERSION: u32 = 3;

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), String> {
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(storage_error)?;
    if version > VERSION {
        return Err("Search database uses a newer schema".into());
    }
    connection
        .execute_batch(database::SCHEMA)
        .map_err(storage_error)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    // Re-read under the write lock: another process may have migrated first.
    let version: u32 = transaction
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(storage_error)?;
    if version > VERSION {
        return Err("Search database uses a newer schema".into());
    }
    if version < 2 {
        add_facets(&transaction, version)?;
        rebuild(&transaction)?;
    }
    if version < VERSION {
        add_choice(&transaction)?;
        transaction
            .pragma_update(None, "user_version", VERSION)
            .map_err(storage_error)?;
    }
    transaction
        .execute_batch(database::FACET_SCHEMA)
        .map_err(storage_error)?;
    transaction.commit().map_err(storage_error)
}

fn add_facets(connection: &Connection, version: u32) -> Result<(), String> {
    if version >= 2 {
        return Ok(());
    }
    let has_kind = connection
        .prepare("SELECT name FROM pragma_table_info('documents') WHERE name='kind'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(storage_error)?;
    if has_kind {
        return Ok(());
    }
    connection.execute_batch(
        "ALTER TABLE documents ADD COLUMN kind TEXT NOT NULL DEFAULT '';
         ALTER TABLE documents ADD COLUMN account TEXT NOT NULL DEFAULT '';
         ALTER TABLE documents ADD COLUMN project TEXT;
         ALTER TABLE documents ADD COLUMN parent_id TEXT;
         ALTER TABLE documents ADD COLUMN refreshed_at INTEGER NOT NULL DEFAULT 0;
         UPDATE documents SET kind=json_extract(json,'$.kind'),parent_id=json_extract(json,'$.parent_id'),account=json_extract(scope,'$.account'),refreshed_at=refreshed;",
    ).map_err(storage_error)
}

/// Version 3 records whether the person chose a board's policy, so a
/// board's declared default applies only until they do. A store whose policy
/// already differs from the default was chosen.
fn add_choice(connection: &Connection) -> Result<(), String> {
    let has_choice = connection
        .prepare("SELECT name FROM pragma_table_info('settings') WHERE name='chosen'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(storage_error)?;
    if !has_choice {
        connection
            .execute_batch("ALTER TABLE settings ADD COLUMN chosen INTEGER NOT NULL DEFAULT 0;")
            .map_err(storage_error)?;
    }
    let default = serde_json::to_string(&crate::Policy::default()).map_err(storage_error)?;
    connection
        .execute("UPDATE settings SET chosen=(policy<>?1)", [default])
        .map_err(storage_error)?;
    Ok(())
}

fn rebuild(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
        DROP TRIGGER IF EXISTS delete_document;
        DROP TABLE IF EXISTS text_index;
        CREATE VIRTUAL TABLE text_index USING fts5(title,body,metadata,
            tokenize='unicode61 remove_diacritics 2', prefix='2 3');
        INSERT INTO text_index(rowid,title,body,metadata)
            SELECT rowid,json_extract(json,'$.title'),json_extract(json,'$.body'),
                json_extract(json,'$.metadata') FROM documents;
        CREATE TRIGGER delete_document AFTER DELETE ON documents BEGIN
            DELETE FROM text_index WHERE rowid=old.rowid;
        END;
    ",
        )
        .map_err(storage_error)
}
