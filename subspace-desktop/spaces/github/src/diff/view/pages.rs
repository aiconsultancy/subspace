//! Loading and validating pinned file continuations.

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::{
    Error,
    diff::{DiffFilePage, V1DiffFilePage},
};
use subspace_framework::dashboard::Toast;

use super::super::model::{FileView, parse_patch};
use super::{DiffEvent, DiffSource, DiffView, load::diff_error};

impl DiffView {
    pub(super) fn load_more(&mut self, index: usize, cx: &mut Context<Self>) {
        let (DiffSource::Fleet(source), Some(file), Some(at)) = (
            self.source.clone(),
            self.files.get(index),
            self.fleet.fingerprint.clone(),
        ) else {
            return;
        };
        let Some(page) = file.next_page.clone() else {
            return;
        };
        if self.fleet.changed() || self.fleet.loading.contains(&index) {
            return;
        }
        let client = match source.client(cx) {
            Ok(client) => client,
            Err(help) => {
                cx.emit(DiffEvent::Toast(Toast::error(help)));
                return;
            }
        };
        self.fleet.loading.insert(index);
        let (mode, generation) = (self.fleet.mode, self.fleet.generation);
        let read = self.fleet.read.clone();
        let path = file.file.path.clone();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let request = DiffFilePage {
                        path: &path,
                        page: Some(&page),
                        at: &at,
                        side: None,
                    };
                    let result = read.retry(|| client.diff_file(source.request(mode), request))?;
                    validate_page(result, &request)
                })
                .await;
            _ = view.update(cx, |view, cx| {
                view.finish_page(generation, index, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_page(
        &mut self,
        generation: u64,
        index: usize,
        result: Result<V1DiffFilePage, Error>,
        cx: &mut Context<Self>,
    ) {
        if self.fleet.generation != generation {
            return;
        }
        self.fleet.loading.remove(&index);
        match result {
            Ok(page) => {
                self.fleet.busy = None;
                self.fleet.unstable |= page.unstable;
                apply_page(&mut self.files[index], page);
                self.highlights.remove(&index);
                self.rebuild();
            }
            Err(error) => self.fleet_page_error(error, cx),
        }
        cx.notify();
    }

    pub(super) fn fleet_page_error(&mut self, error: Error, cx: &mut Context<Self>) {
        match error {
            Error::DiffReadCancelled => {}
            Error::StaleDiff => self.fleet.stale_page = true,
            Error::Busy(_) => self.fleet.busy = Some(diff_error(error)),
            Error::NotFound => cx.emit(DiffEvent::Toast(Toast::error(
                "This file is no longer available. Refresh the diff.",
            ))),
            other => cx.emit(DiffEvent::Toast(Toast::error(other.to_string()))),
        }
    }
}

fn apply_page(file: &mut FileView, page: V1DiffFilePage) {
    if let Some(patch) = page.file.patch {
        let text = file.file.patch.get_or_insert_with(String::new);
        text.push_str(&patch);
        file.patch = parse_patch(text);
        if !file.binary {
            file.reason = None;
        }
    }
    file.next_page = page.file.patch_next_page;
}

pub(super) fn validate_page(
    page: V1DiffFilePage,
    request: &DiffFilePage<'_>,
) -> Result<V1DiffFilePage, Error> {
    if page.fingerprint != request.at {
        return Err(Error::StaleDiff);
    }
    if page.file.path != request.path
        || (request.page.is_some() && page.file.patch_next_page.as_deref() == request.page)
    {
        return Err(Error::InvalidResponse);
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DiffFilePage, Error, V1DiffFilePage, apply_page, validate_page};
    use crate::diff::view::source::fleet_file;

    fn page(fingerprint: &str, patch: &str, next: Option<&str>) -> V1DiffFilePage {
        serde_json::from_value(json!({ "fingerprint": fingerprint, "file": {
            "path": "notes.txt", "status": "added", "additions": 1, "deletions": 0,
            "patch": patch, "patchNextPage": next,
        } }))
        .unwrap()
    }

    #[test]
    fn joins_patch_pages_before_parsing_and_rejects_a_changed_checkout() {
        let request = DiffFilePage {
            path: "notes.txt",
            page: Some("next"),
            at: "snapshot",
            side: None,
        };
        let mut file = fleet_file(page("snapshot", "@@ -0,0 +1 @@\n+exa", Some("next")).file);
        let mismatch = validate_page(page("new", "mple.com\n", None), &request);
        assert_eq!(mismatch.err(), Some(Error::StaleDiff));
        let page = validate_page(page("snapshot", "mple.com\n", None), &request).unwrap();
        apply_page(&mut file, page);
        assert_eq!(file.patch.lines[0].text, "example.com");
        assert!(file.next_page.is_none());
    }
}
