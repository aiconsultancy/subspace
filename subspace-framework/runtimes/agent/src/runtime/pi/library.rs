//! The session index of the local engines that keep their own, Pi and ACP.
use super::{HashMap, Path, fs, state};
use crate::SessionSummary;
use crate::runtime::names;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::runtime) struct LibraryItem {
    pub(in crate::runtime) title: String,
    pub(in crate::runtime) path: String,
    pub(in crate::runtime) modified_at: String,
    pub(in crate::runtime) created_at: String,
}

#[derive(Default, Serialize, Deserialize)]
pub(in crate::runtime) struct Library {
    #[serde(default)]
    pub(in crate::runtime) items: HashMap<String, LibraryItem>,
}

impl Library {
    pub(in crate::runtime) fn summaries(&self) -> Vec<SessionSummary> {
        let mut items: Vec<_> = self
            .items
            .iter()
            .map(|(id, item)| SessionSummary {
                id: id.clone(),
                title: item.title.clone(),
                modified: names::relative_time(&item.modified_at, chrono::Utc::now()),
                modified_at: item.modified_at.clone(),
                created_at: item.created_at.clone(),
            })
            .collect();
        items.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
        items
    }
}

pub(in crate::runtime) fn load(path: &Path) -> Library {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(in crate::runtime) fn save(library: &Library, path: &Path) -> Result<(), String> {
    let text = serde_json::to_string(library)
        .map_err(|_| "Cannot encode the chat library.".to_string())?;
    state::write_atomic(path, &text).map_err(|_| "Cannot save the chat library.".to_string())
}
