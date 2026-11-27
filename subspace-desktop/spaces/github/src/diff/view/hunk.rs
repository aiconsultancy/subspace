//! The unchanged GitHub text behind an expandable hunk gap.

use std::sync::Arc;

use crate::diff::model::FileStatus;
use gpui_kit::{AppContext as _, Context};
use subspace_framework::dashboard::Toast;

use super::{DiffEvent, DiffView, Text};

impl DiffView {
    pub(super) fn expand(&mut self, file: usize, gap: usize, cx: &mut Context<Self>) {
        match self.texts.get(&file) {
            Some(Text::Ready(_)) => {
                self.expanded.insert((file, gap));
                self.rebuild();
                cx.notify();
            }
            Some(Text::Loading) => {}
            None => self.load_text(file, gap, cx),
        }
    }

    fn load_text(&mut self, file: usize, gap: usize, cx: &mut Context<Self>) {
        let (Some((client, login, repo)), Some(view)) =
            (self.github_connection(), self.files.get(file))
        else {
            return;
        };
        let (sha, path) = if view.file.status == FileStatus::Removed {
            (self.base_sha.clone(), view.file.path.clone())
        } else {
            (self.head_sha.clone(), view.file.path.clone())
        };
        self.texts.insert(file, Text::Loading);
        cx.spawn(async move |view, cx| {
            let shown = path.clone();
            let result = cx
                .background_spawn(async move { client.file(&login, &repo, &sha, &path) })
                .await;
            _ = view.update(cx, |view, cx| {
                match result {
                    Ok(text) => {
                        let lines: Vec<String> = text
                            .text
                            .split_terminator('\n')
                            .map(str::to_string)
                            .collect();
                        view.texts.insert(file, Text::Ready(Arc::new(lines)));
                        view.expanded.insert((file, gap));
                        view.rebuild();
                    }
                    Err(error) => {
                        view.texts.remove(&file);
                        cx.emit(DiffEvent::Toast(Toast::error(format!(
                            "Cannot load {shown}: {error}"
                        ))));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
