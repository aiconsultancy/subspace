//! Installing and removing packages, from the Space's toolbar and empty
//! state. Both ask first (a file dialog, a confirmation), then work off the
//! UI thread.

use gpui_kit::{AppContext as _, Context, PathPromptOptions, PromptLevel, Window};

use crate::TsxSpaces;
use crate::store;

// qual:allow(coupling, oi) reason: "Installing and removing change the package list and selection the Space renders; the file work is in store and tested there."
impl TsxSpaces {
    /// Asks for a package file and installs it.
    pub(crate) fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paths) = self.paths() else {
            return;
        };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Install".into()),
        });
        let roots = self.saved.roots.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(files))) = picked.await else {
                return;
            };
            let Some(file) = files.into_iter().next() else {
                return;
            };
            let outcome = cx
                .background_spawn(async move { store::install(&paths, &roots, &file) })
                .await;
            this.update_in(cx, |this, window, cx| this.installed(outcome, window, cx))
                .ok();
        })
        .detach();
    }

    fn installed(
        &mut self,
        outcome: Result<store::Verified, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(verified) => {
                self.notice = None;
                self.selected = self
                    .paths()
                    .map(|paths| paths.package_file(&verified.identity.app_id));
                self.refresh(true, window, cx);
            }
            Err(reason) => {
                self.notice = Some(format!("The package was not installed. {reason}"));
                cx.notify();
            }
        }
    }

    /// Asks, then removes the selected package and, if a trusted root
    /// authenticates it, its data.
    pub(crate) fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(paths), Some(package)) = (self.paths(), self.selected_package().cloned()) else {
            return;
        };
        let name = package.state.as_ref().map_or_else(
            |_| package.file.display().to_string(),
            |verified| verified.name.clone(),
        );
        // The same check Remove makes decides what the prompt promises.
        let removal = store::removal(&paths, &package);
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Remove {name}?"),
            Some(&removal.detail()),
            &["Remove", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let file = package.file.clone();
            let app_id = package.app_id.clone();
            let outcome = cx
                .background_spawn(async move { store::remove(&paths, &file, app_id.as_deref()) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.notice = outcome.err();
                this.selected = None;
                this.refresh(true, window, cx);
            })
            .ok();
        })
        .detach();
    }
}
