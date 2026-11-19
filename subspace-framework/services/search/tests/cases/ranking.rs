use super::{document, enable, query, setup};
use subspace_search::{Hit, Query, Ranking, Reranker, RerankerStatus, Service};

struct TestReranker {
    service: Service,
    mode: u8,
}

impl Reranker for TestReranker {
    fn rerank(&self, _: &str, hits: &[Hit]) -> Result<Vec<Hit>, String> {
        assert_eq!(hits.len(), 1);
        let mut hits = hits.to_vec();
        match self.mode {
            0 => {
                self.service.clear("irc")?;
            }
            1 => {
                hits[0].target.id = "invented".into();
            }
            _ => {
                hits[0].document.body = "invented".into();
                hits[0].score = 42.0;
            }
        }
        Ok(hits)
    }
    fn status(&self) -> RerankerStatus {
        RerankerStatus {
            installed: true,
            name: "test".into(),
            download_bytes: 0,
            message: "fixture".into(),
        }
    }
    fn install(&self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn reranking_revalidates_after_clear_and_cannot_fabricate_targets_or_text() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(&scope, &[document("real", "hello")])
        .unwrap();
    let query = Query {
        ranking: Ranking::Reranked,
        ..query("hello")
    };
    assert!(service.search(&query).unwrap().warning.is_some());
    for mode in [1, 2, 0] {
        service.set_reranker(std::sync::Arc::new(TestReranker {
            service: service.clone(),
            mode,
        }));
        let response = service.search(&query).unwrap();
        if mode == 0 {
            assert!(response.hits.is_empty());
        } else {
            assert_eq!(response.hits[0].target.id, "real");
            assert_eq!(response.hits[0].document.body, "hello");
            assert_eq!(
                response.ranking,
                if mode == 1 { "bm25" } else { "reranked" }
            );
        }
    }
    assert_eq!(service.reranker_status().unwrap().name, "test");
    assert!(service.install_reranker().is_ok());
}

#[test]
fn bm25_prefers_title_and_responses_are_bounded_without_truncating_cached_payload() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let mut title = document("title", "other");
    title.title = "needle".into();
    service
        .upsert(&scope, &[document("body", "needle"), title])
        .unwrap();
    assert_eq!(
        service.search(&query("needle")).unwrap().hits[0].target.id,
        "title"
    );
    let body = "needle ".repeat(3000);
    service.upsert(&scope, &[document("long", &body)]).unwrap();
    let response = service.search(&query("needle")).unwrap();
    assert!(response.truncated);
    assert!(serde_json::to_vec(&response).unwrap().len() < 1024 * 1024);
    let long = response
        .hits
        .iter()
        .find(|hit| hit.target.id == "long")
        .unwrap();
    assert!(long.document.body.len() <= 8192);
    assert!(long.document.payload.is_none());
    assert_eq!(service.read(&long.target).unwrap().unwrap().body, body);
}
