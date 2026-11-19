use std::collections::HashSet;

pub const VIEWED_KEY: &str = "github.viewed";
const VIEWED_PRS: usize = 50;

pub fn viewed_id(repo: &str, number: u32, head_sha: &str) -> String {
    format!("{repo}#{number}@{head_sha}")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewedStore(pub Vec<(String, Vec<String>)>);

impl ViewedStore {
    pub fn parse(json: Option<&str>) -> Self {
        Self(
            json.and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or_default(),
        )
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".into())
    }

    pub fn get(&self, id: &str) -> HashSet<String> {
        self.0
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, paths)| paths.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn with(mut self, id: &str, paths: Vec<String>) -> Self {
        self.0.retain(|(known, _)| known != id);
        if !paths.is_empty() {
            self.0.push((id.to_string(), paths));
        }
        let start = self.0.len().saturating_sub(VIEWED_PRS);
        self.0.drain(..start);
        self
    }
}
