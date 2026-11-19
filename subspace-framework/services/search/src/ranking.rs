use crate::{Hit, Query, Ranking, Reranker};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) fn rank(
    reranker: Option<Arc<dyn Reranker>>,
    query: &Query,
    candidates: &[Hit],
) -> (Vec<Hit>, String, Option<String>) {
    if query.ranking == Ranking::Bm25 {
        return (candidates.to_vec(), "bm25".into(), None);
    }
    let Some(reranker) = reranker else {
        return fallback(candidates, "Local reranker is unavailable; using BM25");
    };
    match reranker.rerank(&query.text, candidates) {
        Ok(ranked) => match validated_ranking(candidates, ranked) {
            Some(hits) => (hits, "reranked".into(), None),
            None => fallback(
                candidates,
                "Local reranker returned invalid candidates; using BM25",
            ),
        },
        Err(_) => fallback(
            candidates,
            "Local reranking failed or is not installed; using BM25",
        ),
    }
}

fn validated_ranking(candidates: &[Hit], ranked: Vec<Hit>) -> Option<Vec<Hit>> {
    if ranked.len() != candidates.len() {
        return None;
    }
    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .map(|hit| {
            let index = candidates
                .iter()
                .position(|candidate| candidate.target == hit.target)?;
            if !seen.insert(index) || !hit.score.is_finite() {
                return None;
            }
            let mut original = candidates[index].clone();
            original.score = hit.score;
            Some(original)
        })
        .collect()
}

fn fallback(hits: &[Hit], message: &str) -> (Vec<Hit>, String, Option<String>) {
    (hits.to_vec(), "bm25".into(), Some(message.into()))
}
