//! Optional, local-only cross-encoder reranking. Construction and search never download.
//! Windows keeps the static C runtime and uses BM25 without this backend.
#![cfg(not(windows))]

mod assets;
mod ranking;

use fastembed::TextRerank;
use std::{path::PathBuf, sync::Mutex};
use subspace_search::{Hit, Reranker, RerankerStatus};

/// One lazy CPU session shared by the application's search service.
// qual:api
pub struct LocalReranker {
    root: PathBuf,
    model: Mutex<Option<TextRerank>>,
}

impl LocalReranker {
    /// `cache_dir` must be an app-owned private directory, never a service cache.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            root: cache_dir.join(assets::DIRECTORY),
            model: Mutex::new(None),
        }
    }
}

impl Reranker for LocalReranker {
    fn rerank(&self, query: &str, hits: &[Hit]) -> Result<Vec<Hit>, String> {
        ranking::validate_input(query, hits)?;
        if hits.is_empty() {
            return Ok(Vec::new());
        }
        let mut session = self
            .model
            .try_lock()
            .map_err(|_| "Local reranker is busy or unavailable.")?;
        if session.is_none() {
            *session = Some(ranking::load(&self.root)?);
        }
        let model = session.as_mut().ok_or("Local model is unavailable.")?;
        ranking::infer(model, query, hits)
    }

    fn status(&self) -> RerankerStatus {
        let installed = assets::present(&self.root);
        RerankerStatus {
            installed,
            name: assets::NAME.into(),
            download_bytes: assets::total_bytes(),
            message: if installed {
                "Installed locally; checksum validation and CPU initialization occur on first rerank."
            } else {
                "Model missing. Explicit installation downloads 92 MB of public assets; searches remain local."
            }.into(),
        }
    }

    fn install(&self) -> Result<(), String> {
        let mut session = self
            .model
            .try_lock()
            .map_err(|_| "Local reranker is busy or unavailable.")?;
        assets::install(&self.root)?;
        *session = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LocalReranker;
    use subspace_search::Reranker;

    #[test]
    fn construction_status_and_empty_search_do_not_create_cache() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("not-created");
        let model = LocalReranker::new(root.clone());
        assert!(!model.status().installed);
        assert_eq!(model.status().download_bytes, 91_728_555);
        assert!(model.rerank("query", &[]).unwrap().is_empty());
        assert!(!root.exists());
    }
}
