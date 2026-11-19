use rusqlite::{OptionalExtension, params};

use crate::{
    IndexMetadata, ReferenceIdentity, ReferenceMatch, RelatedItem, RelatedQuery, Scope, Target,
    database, service::require_eligible, storage_error,
};

const MAX_REFERENCES: usize = 64;
/// The most documents one metadata write covers, as one upsert batch does.
const MAX_BATCH: usize = 100;
const MAX_VALUE: usize = 1024;
const MAX_RELATION_TYPE: usize = 128;
const MAX_RELATED: usize = 100;
const MAX_FILTERS: usize = 32;
const ID_COLUMN: usize = 0;
const DOCUMENT_COLUMN: usize = 1;
const RELATION_COLUMN: usize = 2;
const EXACT_COLUMN: usize = 3;
const EVIDENCE_COLUMN: usize = 4;
const CONFIDENCE_COLUMN: usize = 5;

pub(crate) fn write(
    store: &mut database::Store,
    scope: &Scope,
    entries: &[(String, IndexMetadata)],
) -> Result<(), String> {
    if entries.len() > MAX_BATCH {
        return Err("Search index metadata exceeds its bounds".into());
    }
    for (id, metadata) in entries {
        validate(metadata, id)?;
    }
    let transaction = store.connection.transaction().map_err(storage_error)?;
    require_eligible(&transaction, scope)?;
    let scope_key = database::key(scope)?;
    for (id, metadata) in entries {
        write_one(&transaction, &scope_key, id, metadata)?;
    }
    transaction.commit().map_err(storage_error)
}

fn write_one(
    transaction: &rusqlite::Transaction<'_>,
    scope_key: &str,
    id: &str,
    metadata: &IndexMetadata,
) -> Result<(), String> {
    let exists = transaction
        .query_row(
            "SELECT 1 FROM documents WHERE scope=?1 AND id=?2",
            params![scope_key, id],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage_error)?
        .is_some();
    if !exists {
        return Err("Search document is not retained in the active scope".into());
    }
    transaction
        .execute(
            "UPDATE documents SET project=?3 WHERE scope=?1 AND id=?2",
            params![scope_key, id, metadata.project],
        )
        .map_err(storage_error)?;
    transaction
        .execute(
            "DELETE FROM \"references\" WHERE scope=?1 AND document_id=?2",
            params![scope_key, id],
        )
        .map_err(storage_error)?;
    for reference in &metadata.references {
        let (exact, evidence, confidence) = match &reference.matched {
            ReferenceMatch::Exact => (true, None, None),
            ReferenceMatch::Heuristic {
                evidence,
                confidence,
            } => (false, Some(evidence.as_str()), Some(*confidence)),
        };
        transaction.execute(
            "INSERT INTO \"references\"(scope,document_id,identity,relation_type,exact,evidence,confidence) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![scope_key, id, reference.identity.as_str(), reference.relation_type, exact, evidence, confidence],
        ).map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) fn related(
    stores: &std::collections::BTreeMap<String, database::Store>,
    reference: &ReferenceIdentity,
    query: &RelatedQuery,
) -> Result<Vec<RelatedItem>, String> {
    validate_query(query)?;
    for board in &query.boards {
        crate::service::store(stores, board)?;
    }
    let mut items = Vec::new();
    for (board, store) in stores
        .iter()
        .filter(|(board, _)| query.boards.is_empty() || query.boards.contains(board))
    {
        items.extend(board_related(board, store, reference, query)?);
    }
    items.truncate(query.limit);
    Ok(items)
}

fn board_related(
    board: &str,
    store: &database::Store,
    reference: &ReferenceIdentity,
    query: &RelatedQuery,
) -> Result<Vec<RelatedItem>, String> {
    let policy = database::policy(&store.connection)?;
    let (active, _) = database::selected(&store.connection)?;
    let Some(scope_key) = active.filter(|_| policy.enabled) else {
        return Ok(Vec::new());
    };
    let scope: Scope = serde_json::from_str(&scope_key).map_err(storage_error)?;
    let mut statement = store.connection.prepare(
        "SELECT d.id,d.json,r.relation_type,r.exact,r.evidence,r.confidence FROM \"references\" r JOIN documents d ON d.scope=r.scope AND d.id=r.document_id WHERE r.scope=?1 AND r.identity=?2 AND d.refreshed>=?3 AND d.refreshed<=?4 ORDER BY d.refreshed DESC,d.id",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(
            params![
                scope_key,
                reference.as_str(),
                database::cutoff(&policy),
                crate::now()
            ],
            |row| {
                Ok(RelatedRow {
                    id: row.get(ID_COLUMN)?,
                    document: row.get(DOCUMENT_COLUMN)?,
                    relation_type: row.get(RELATION_COLUMN)?,
                    exact: row.get(EXACT_COLUMN)?,
                    evidence: row.get(EVIDENCE_COLUMN)?,
                    confidence: row.get(CONFIDENCE_COLUMN)?,
                })
            },
        )
        .map_err(storage_error)?;
    rows.filter_map(|row| match row {
        Ok(row)
            if query.relation_types.is_empty()
                || query.relation_types.contains(&row.relation_type) =>
        {
            Some(decode(board, &scope, row))
        }
        Ok(_) => None,
        Err(error) => Some(Err(storage_error(error))),
    })
    .take(query.limit)
    .collect()
}

struct RelatedRow {
    id: String,
    document: String,
    relation_type: String,
    exact: bool,
    evidence: Option<String>,
    confidence: Option<f32>,
}

fn decode(_board: &str, scope: &Scope, row: RelatedRow) -> Result<RelatedItem, String> {
    let document = serde_json::from_str(&row.document).map_err(storage_error)?;
    let matched = if row.exact {
        ReferenceMatch::Exact
    } else {
        ReferenceMatch::Heuristic {
            evidence: row.evidence.unwrap_or_default(),
            confidence: row.confidence.unwrap_or_default(),
        }
    };
    Ok(RelatedItem {
        hit: crate::Hit {
            target: Target {
                scope: scope.clone(),
                id: row.id,
            },
            document,
            excerpt: String::new(),
            score: 0.0,
        },
        relation_type: row.relation_type,
        matched,
    })
}

fn validate(metadata: &IndexMetadata, id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > MAX_VALUE
        || metadata.project.as_ref().is_some_and(|value| value.is_empty() || value.len() > MAX_VALUE)
        || metadata.references.len() > MAX_REFERENCES
        || metadata.references.iter().any(|reference| {
            reference.relation_type.is_empty()
                || reference.relation_type.len() > MAX_RELATION_TYPE
                || matches!(&reference.matched, ReferenceMatch::Heuristic { evidence, confidence } if evidence.is_empty() || evidence.len() > MAX_VALUE || !(0.0..=1.0).contains(confidence))
        })
    {
        return Err("Search index metadata exceeds its bounds".into());
    }
    Ok(())
}

fn validate_query(query: &RelatedQuery) -> Result<(), String> {
    if !(1..=MAX_RELATED).contains(&query.limit)
        || query.boards.len() > MAX_FILTERS
        || query.relation_types.len() > MAX_FILTERS
        || query
            .boards
            .iter()
            .chain(&query.relation_types)
            .any(|value| value.is_empty() || value.len() > MAX_RELATION_TYPE)
    {
        return Err("Related-items query exceeds its bounds".into());
    }
    Ok(())
}
