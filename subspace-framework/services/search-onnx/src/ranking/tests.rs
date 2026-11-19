use super::{apply_scores, document_text, validate_input};
use crate::LocalReranker;
use fastembed::RerankResult;
use subspace_search::{Document, Hit, Reranker, Scope, Target};

fn hit(id: &str, body: &str) -> Hit {
    Hit {
        target: Target {
            scope: Scope {
                app: "synthetic".into(),
                board: "fixture".into(),
                endpoint: "offline".into(),
                tenant: "fixture".into(),
                account: "fixture".into(),
            },
            id: id.into(),
        },
        document: Document {
            id: id.into(),
            title: String::new(),
            body: body.into(),
            metadata: String::new(),
            parent_id: None,
            kind: "fixture".into(),
            url: None,
            navigation: id.into(),
            refreshed_at: 0,
            payload: None,
        },
        excerpt: body.into(),
        score: 0.0,
    }
}

fn score(index: usize, score: f32) -> RerankResult {
    RerankResult {
        index,
        score,
        document: None,
    }
}

#[test]
fn preserves_exact_targets_and_stable_ties() {
    let hits = vec![hit("a", "one"), hit("b", "two"), hit("c", "three")];
    let result = apply_scores(&hits, vec![score(2, 1.0), score(1, 2.0), score(0, 1.0)]).unwrap();
    assert_eq!(
        result
            .iter()
            .map(|hit| hit.target.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a", "c"]
    );
    assert_eq!(result[0].document.navigation, "b");
    assert_eq!(result[0].target.scope, hits[1].target.scope);
    assert_eq!(result[0].score, 2.0);
}

#[test]
fn rejects_invalid_model_output() {
    let hits = vec![hit("a", "one"), hit("b", "two")];
    assert!(apply_scores(&hits, vec![score(0, 1.0)]).is_err());
    assert!(apply_scores(&hits, vec![score(0, 1.0), score(0, 2.0)]).is_err());
    assert!(apply_scores(&hits, vec![score(0, 1.0), score(2, 2.0)]).is_err());
    assert!(apply_scores(&hits, vec![score(0, 1.0), score(1, f32::NAN)]).is_err());
}

#[test]
fn bounds_unicode_input_and_shortlist() {
    assert!(validate_input(" ", &[]).is_err());
    assert!(validate_input(&"é".repeat(1025), &[]).is_err());
    assert!(validate_input("query", &vec![hit("a", "body"); 101]).is_ok());
    assert!(validate_input("query", &vec![hit("a", "body"); 129]).is_err());
    let text = document_text(&hit("a", &"🦀".repeat(10000)));
    assert_eq!(text.chars().count(), 8193);
}

#[test]
fn missing_model_fails_without_creating_any_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("absent");
    let model = LocalReranker::new(root.clone());
    assert!(model.rerank("query", &[hit("a", "body")]).is_err());
    assert!(!root.exists());
}

#[test]
fn concurrent_use_falls_back_without_waiting_or_installing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("absent");
    let model = LocalReranker::new(root.clone());
    let _busy = model.model.lock().unwrap();
    assert!(
        model
            .rerank("query", &[hit("a", "body")])
            .unwrap_err()
            .contains("busy")
    );
    assert!(model.install().unwrap_err().contains("busy"));
    assert!(!root.exists());
}

#[test]
#[ignore = "explicit public model download and native ONNX inference; supply SUBSPACE_ONNX_PROBE_CACHE"]
fn public_model_native_probe() {
    let root = std::env::var_os("SUBSPACE_ONNX_PROBE_CACHE")
        .expect("Set SUBSPACE_ONNX_PROBE_CACHE to an app-owned synthetic probe directory");
    let model = LocalReranker::new(root.into());
    model.install().expect("fixed public model install");
    assert!(model.status().installed);
    let hits = vec![
        hit("irrelevant", "Bananas grow in warm tropical climates."),
        hit(
            "relevant",
            "A lost password can be reset using the account recovery page.",
        ),
    ];
    let result = model
        .rerank("How do I reset a forgotten password?", &hits)
        .expect("native inference");
    assert_eq!(result[0].target.id, "relevant");
    assert!(result[0].score > result[1].score);
    assert_eq!(result.len(), 2);
    eprintln!(
        "Native ONNX scores: relevant={}, irrelevant={}",
        result[0].score, result[1].score
    );
}
