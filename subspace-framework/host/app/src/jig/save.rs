//! Named-file confirmation and the separate active-preference save boundary.

use std::path::{Path, PathBuf};

use gpui_kit::component::select::SearchableVec;
use gpui_kit::{AppContext as _, Context, Window};

use super::Jig;
use crate::theme::{self, AppearanceSnapshot, FontChoices, ThemeDocument, storage};
use subspace_dashboard::density::DensityPreview;

#[derive(Clone)]
pub(super) struct PendingSave {
    pub name: String,
    pub json: String,
    pub expected: String,
}

enum SaveOutcome {
    Confirm(PendingSave),
    Saved(PathBuf, String),
}

impl Jig {
    pub(super) fn named_document(
        &self,
        cx: &gpui_kit::App,
    ) -> Result<(String, ThemeDocument), String> {
        let name = self.name.read(cx).value().to_string();
        storage::theme_filename(&name)?;
        let document = self.edited_document(cx)?.named(&name)?;
        theme::validate_custom_theme(&document.to_json()?)?;
        Ok((name, document))
    }

    pub(super) fn save(&mut self, confirm: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = self.save_request(confirm, cx);
        let (directory, name, json, expected) = match result {
            Ok(request) => request,
            Err(error) => {
                self.status = error.into();
                cx.notify();
                return;
            }
        };
        let names = theme::theme_names(cx);
        let generation = self.generation;
        self.busy = true;
        self.status = "Saving theme...".into();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let saved_name = name.clone();
            let result = cx
                .background_spawn(async move {
                    prepare_or_save(&directory, &name, &json, expected.as_deref(), &names)
                })
                .await;
            _ = this.update_in(cx, |jig, window, cx| {
                if jig.generation == generation {
                    jig.finish_save(&saved_name, result, window, cx);
                }
            });
        })
        .detach();
    }

    fn save_request(
        &self,
        confirm: bool,
        cx: &gpui_kit::App,
    ) -> Result<(PathBuf, String, String, Option<String>), String> {
        let directory = theme::theme_directory(cx)?;
        if confirm {
            let pending = self
                .pending
                .as_ref()
                .ok_or("Overwrite confirmation expired. Save again.")?;
            return Ok((
                directory,
                pending.name.clone(),
                pending.json.clone(),
                Some(pending.expected.clone()),
            ));
        }
        let (name, document) = self.named_document(cx)?;
        Ok((directory, name, document.to_json()?, None))
    }

    fn finish_save(
        &mut self,
        name: &str,
        result: Result<SaveOutcome, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        match result {
            Ok(SaveOutcome::Confirm(pending)) => {
                self.status = format!(
                    "{} already exists. Confirm overwrite below, or choose another name.",
                    pending.name
                )
                .into();
                self.pending = Some(pending);
            }
            Ok(SaveOutcome::Saved(path, json)) => {
                self.activate_saved(name, &path, &json, window, cx)
            }
            Err(error) => {
                self.pending = None;
                self.status = error.into();
            }
        }
        cx.notify();
    }

    fn activate_saved(
        &mut self,
        name: &str,
        path: &Path,
        json: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending = None;
        let warnings = theme::reload(cx);
        if !activation_matches(json, theme::document(name, cx)) {
            self.status = format!(
                "Theme file saved to {}. Active appearance was not saved: activation refused.",
                path.display()
            )
            .into();
            return;
        }
        if let Err(error) = theme::save_selection(name, &FontChoices::default(), cx) {
            self.status = format!(
                "Theme file saved to {}. Active appearance was not saved: {error}",
                path.display()
            )
            .into();
            return;
        }
        cx.set_global(DensityPreview(false));
        self.snapshot = Some(AppearanceSnapshot::capture(cx));
        cx.set_global(DensityPreview(true));
        self.draft = theme::document(name, cx).ok();
        self.syncing = true;
        self.base.update(cx, |base, cx| {
            base.set_items(SearchableVec::new(theme::theme_names(cx)), window, cx);
            base.set_selected_value(&name.to_owned(), window, cx);
        });
        self.sync_fields(&FontChoices::default(), window, cx);
        self.syncing = false;
        self.status = format!("Saved {}. {}", path.display(), warnings.join("\n")).into();
        window.refresh();
    }
}

fn prepare_or_save(
    directory: &Path,
    name: &str,
    json: &str,
    expected: Option<&str>,
    names: &[String],
) -> Result<SaveOutcome, String> {
    let path = storage::target(directory, name)?;
    if expected.is_none() {
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                return Ok(SaveOutcome::Confirm(PendingSave {
                    name: name.to_owned(),
                    json: json.to_owned(),
                    expected: storage::read_theme(&path)?,
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if names
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(name))
                {
                    return Err(
                        "That name belongs to another theme file. Choose a new name.".into(),
                    );
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    storage::save(directory, name, json, expected)
        .map(|path| SaveOutcome::Saved(path, json.to_owned()))
}

fn activation_matches(json: &str, loaded: Result<ThemeDocument, String>) -> bool {
    let Ok(saved) = ThemeDocument::parse(json) else {
        return false;
    };
    loaded.ok().as_ref() == Some(&saved)
}

#[cfg(test)]
mod tests {
    use super::{SaveOutcome, prepare_or_save};

    #[test]
    fn an_existing_target_requires_confirmation_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let json = crate::theme::THEME_JSON.replace(crate::theme::THEME_NAME, "My Theme");
        let path = crate::theme::storage::save(dir.path(), "My Theme", &json, None).unwrap();
        let replacement = json.replace("#282c34", "#222222");
        let SaveOutcome::Confirm(pending) =
            prepare_or_save(dir.path(), "My Theme", &replacement, None, &[]).unwrap()
        else {
            panic!("confirmation required");
        };
        assert_eq!(pending.expected, json);
        assert_eq!(std::fs::read_to_string(path).unwrap(), json);
        assert!(matches!(
            prepare_or_save(
                dir.path(),
                "My Theme",
                &replacement,
                Some(&pending.expected),
                &[]
            )
            .unwrap(),
            SaveOutcome::Saved(_, _)
        ));
    }

    #[test]
    fn a_name_owned_by_another_file_cannot_create_a_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        assert!(prepare_or_save(dir.path(), "Existing", "{}", None, &["existing".into()]).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn activation_refuses_a_reloaded_document_that_does_not_match() {
        let json = serde_json::json!({
            "name": "Mine",
            "themes": [{
                "name": "Mine",
                "mode": "dark",
                "colors": { "background": "#101115" }
            }]
        })
        .to_string();
        let saved = crate::theme::ThemeDocument::parse(&json).unwrap();
        assert!(super::activation_matches(&json, Ok(saved.clone())));
        let other = crate::theme::ThemeDocument::parse(&json.replace("Mine", "Other")).unwrap();
        assert!(!super::activation_matches(&json, Ok(other)));
        assert!(!super::activation_matches(&json, Err("missing".into())));
    }
}
