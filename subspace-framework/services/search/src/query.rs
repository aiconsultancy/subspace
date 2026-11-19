use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::{
    Document, Hit, Query, Scope, SearchResponse, Service, Target, database, limits, service::store,
    storage_error,
};

mod filters;

pub(crate) use filters::{push_facets, validate as validate_filters};

const CANDIDATES: usize = 101;
const RESPONSE_BYTES: usize = 1024 * 1024;

pub(crate) fn search(
    service: &Service,
    query: &Query,
    reranker: Option<std::sync::Arc<dyn crate::Reranker>>,
) -> Result<SearchResponse, String> {
    search_filtered(service, query, &crate::Filters::default(), reranker)
}

pub(crate) fn search_filtered(
    service: &Service,
    query: &Query,
    filters: &crate::Filters,
    reranker: Option<std::sync::Arc<dyn crate::Reranker>>,
) -> Result<SearchResponse, String> {
    let expression = literal(query)?;
    filters::validate(filters)?;
    let (candidates, generations) = candidates(service, query, filters, &expression)?;
    let (ranked, ranking, warning) = crate::ranking::rank(reranker, query, &candidates);
    let sources = service.sources()?;
    let hits = revalidate(service, &ranked, &generations)?;
    let source_bytes = serde_json::to_vec(&sources).map_err(storage_error)?.len();
    let budget = RESPONSE_BYTES
        .saturating_sub(source_bytes)
        .saturating_sub(limits::RESPONSE_RESERVE);
    let (hits, truncated) = bound(hits, query.limit, budget);
    Ok(SearchResponse {
        hits,
        sources,
        ranking,
        warning,
        truncated,
    })
}

fn candidates(
    service: &Service,
    query: &Query,
    filters: &crate::Filters,
    expression: &str,
) -> Result<(Vec<Hit>, BTreeMap<String, i64>), String> {
    let mut stores = service.stores()?;
    for board in &query.boards {
        store(&stores, board)?;
    }
    let mut hits = Vec::new();
    let mut generations = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    for (board, store) in stores
        .iter_mut()
        .filter(|(board, _)| query.boards.is_empty() || query.boards.contains(board))
    {
        let deadline = crate::deadline::Deadline::install(store, deadline)?;
        let snapshot = deadline
            .store()
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        generations.insert(board.clone(), database::selected(&snapshot)?.1);
        hits.extend(find(&snapshot, expression, filters)?);
        hits.sort_by(|left, right| left.score.total_cmp(&right.score));
        hits.truncate(CANDIDATES);
    }
    Ok((hits, generations))
}

fn revalidate(
    service: &Service,
    hits: &[Hit],
    generations: &BTreeMap<String, i64>,
) -> Result<Vec<Hit>, String> {
    let stores = service.stores()?;
    let snapshots: BTreeMap<_, _> = stores
        .iter()
        .filter(|(board, _)| generations.contains_key(*board))
        .map(|(board, store)| {
            store
                .connection
                .unchecked_transaction()
                .map(|snapshot| (board.as_str(), snapshot))
                .map_err(storage_error)
        })
        .collect::<Result<_, _>>()?;
    let mut valid = Vec::new();
    for hit in hits {
        let snapshot = snapshots
            .get(hit.target.scope.board.as_str())
            .ok_or("Search scope is unavailable")?;
        if generations.get(&hit.target.scope.board) != Some(&database::selected(snapshot)?.1) {
            continue;
        }
        if database::read(snapshot, &hit.target.scope, &hit.target.id)?.as_ref()
            == Some(&hit.document)
        {
            valid.push(hit.clone());
        }
    }
    Ok(valid)
}
fn literal(query: &Query) -> Result<String, String> {
    if query.text.len() > limits::MAX_QUERY
        || query.limit == 0
        || query.limit > limits::MAX_RESULTS
        || query.boards.len() > limits::MAX_BOARDS
    {
        return Err("Search query exceeds supported bounds".into());
    }
    let text: String = query
        .text
        .chars()
        .filter(|character| !('\u{0300}'..='\u{036f}').contains(character))
        .collect();
    let mut terms: Vec<String> = text
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect();
    if terms.len() > limits::MAX_TERMS {
        return Err("Search query contains too many terms".into());
    }
    if let Some(last) = terms.last_mut() {
        last.push('*');
    }
    Ok(terms.join(" AND "))
}

fn find(
    connection: &rusqlite::Connection,
    expression: &str,
    filters: &crate::Filters,
) -> Result<Vec<Hit>, String> {
    if expression.is_empty() {
        return Ok(Vec::new());
    }
    let policy = database::policy(connection)?;
    let (active, _) = database::selected(connection)?;
    let Some(active) = active.filter(|_| policy.enabled) else {
        return Ok(Vec::new());
    };
    let scope: Scope = serde_json::from_str(&active).map_err(storage_error)?;
    let (sql, values) = filters::sql(expression, &active, &policy, filters);
    let mut statement = connection.prepare(&sql).map_err(storage_error)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(storage_error)?;
    rows.map(|row| decode(row.map_err(storage_error)?, &scope))
        .collect()
}

fn decode(row: (String, String, f64), scope: &Scope) -> Result<Hit, String> {
    let document: Document = serde_json::from_str(&row.0).map_err(storage_error)?;
    Ok(Hit {
        target: Target {
            scope: scope.clone(),
            id: document.id.clone(),
        },
        document,
        excerpt: prefix(&row.1, limits::EXCERPT_BYTES),
        score: row.2,
    })
}

fn bound(hits: Vec<Hit>, limit: usize, budget: usize) -> (Vec<Hit>, bool) {
    let mut truncated = hits.len() > limit;
    let mut bytes = 0_usize;
    let mut bounded = Vec::new();
    for mut hit in hits.into_iter().take(limit) {
        truncated |= hit.document.body.len() > limits::BODY_BYTES || hit.document.payload.is_some();
        hit.document.body = prefix(&hit.document.body, limits::BODY_BYTES);
        hit.document.payload = None;
        bytes = bytes
            .saturating_add(serde_json::to_vec(&hit).map_or(RESPONSE_BYTES, |json| json.len()));
        if bytes > budget {
            truncated = true;
            break;
        }
        bounded.push(hit);
    }
    (bounded, truncated)
}

fn prefix(text: &str, bytes: usize) -> String {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}
