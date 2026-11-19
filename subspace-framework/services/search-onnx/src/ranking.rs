use fastembed::{
    RerankInitOptionsUserDefined, RerankResult, TextRerank, TokenizerFiles,
    UserDefinedRerankingModel,
};
use std::{fs, path::Path};
use subspace_search::Hit;

use crate::assets;

// Includes the service's extra candidate used to report truncation at limit 100.
const MAX_HITS: usize = 128;
const MAX_QUERY_BYTES: usize = 2_048;
const MAX_TEXT_CHARS: usize = 8_192;
const MAX_SEQUENCE_TOKENS: usize = 512;
const MAX_TITLE_CHARS: usize = 512;
const BATCH_SIZE: usize = 8;

pub(crate) fn validate_input(query: &str, hits: &[Hit]) -> Result<(), String> {
    if query.trim().is_empty() || query.len() > MAX_QUERY_BYTES || hits.len() > MAX_HITS {
        return Err(
            "Local reranking requires a nonempty query of at most 2048 bytes and at most 128 candidates."
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn load(root: &Path) -> Result<TextRerank, String> {
    assets::validate(root)?;
    let files = TokenizerFiles {
        tokenizer_file: read(root, "tokenizer.json")?,
        config_file: read(root, "config.json")?,
        special_tokens_map_file: read(root, "special_tokens_map.json")?,
        tokenizer_config_file: read(root, "tokenizer_config.json")?,
    };
    TextRerank::try_new_from_user_defined(
        UserDefinedRerankingModel::new(root.join("model.onnx"), files),
        RerankInitOptionsUserDefined::new()
            .with_max_length(MAX_SEQUENCE_TOKENS)
            .with_intra_threads(2),
    )
    .map_err(|_| "Cannot initialize the installed local ONNX model on this platform.".into())
}

fn read(root: &Path, name: &str) -> Result<Vec<u8>, String> {
    fs::read(root.join(name)).map_err(|_| "Cannot read installed tokenizer.".into())
}

pub(crate) fn infer(model: &mut TextRerank, query: &str, hits: &[Hit]) -> Result<Vec<Hit>, String> {
    let documents: Vec<String> = hits.iter().map(document_text).collect();
    let scores = model
        .rerank(query.to_owned(), &documents, false, Some(BATCH_SIZE))
        .map_err(|_| "Local ONNX inference failed; lexical search remains available.")?;
    apply_scores(hits, scores)
}

fn document_text(hit: &Hit) -> String {
    hit.document
        .title
        .chars()
        .take(MAX_TITLE_CHARS)
        .chain(std::iter::once('\n'))
        .chain(hit.document.body.chars().take(MAX_TEXT_CHARS))
        .collect()
}

fn apply_scores(hits: &[Hit], scores: Vec<RerankResult>) -> Result<Vec<Hit>, String> {
    if scores.len() != hits.len() {
        return Err("Local model returned an invalid result count.".into());
    }
    let mut by_index = vec![None; hits.len()];
    for result in scores {
        let slot = by_index
            .get_mut(result.index)
            .ok_or("Local model returned an invalid result index.")?;
        if slot.is_some() || !result.score.is_finite() {
            return Err("Local model returned invalid scores.".into());
        }
        *slot = Some(f64::from(result.score));
    }
    let mut ranked = hits.to_vec();
    for (hit, score) in ranked.iter_mut().zip(by_index) {
        hit.score = score.ok_or("Local model omitted a result.")?;
    }
    // Stable sort preserves lexical ordering for equal genuine model logits.
    ranked.sort_by(|left, right| right.score.total_cmp(&left.score));
    Ok(ranked)
}

#[cfg(test)]
mod tests;
