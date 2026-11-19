use gpui_kit::{AppContext as _, Context, PathPromptOptions, Window};
use subspace_framework::dashboard::Services;

use super::super::editor::{choice, set};
use super::super::model::SourceKind;
use super::{Manager, bind, request, selected_account};

impl Manager {
    pub(in crate::manager) fn choose_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let folder = editor.source_kind(cx) == SourceKind::Folder;
        let ticket = self.begin(cx);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some(
                if folder {
                    "Choose a built board folder"
                } else {
                    "Choose a board ZIP"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            _ = this.update_in(cx, |this, window, cx| {
                if !this.epoch.accepts(ticket) {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let (Some(editor), Some(path)) = (&this.editor, paths.first()) {
                            set(&editor.location, &path.to_string_lossy(), window, cx);
                        }
                    }
                    Ok(Err(_)) => {
                        this.error =
                            Some("The file picker could not open. Enter a path instead.".into())
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::manager) fn load_releases(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &self.editor else { return };
        let repository = match request::repository(editor.location.read(cx).value().trim()) {
            Ok(repository) => repository,
            Err(error) => {
                self.fail(error, cx);
                return;
            }
        };
        let prereleases = editor.prereleases;
        let Some(services) = cx.try_global::<Services>() else {
            return;
        };
        let credentials = services.credentials.clone();
        let account = selected_account(cx);
        let ticket = self.begin(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let bound = bind(
                        subspace_framework::client_github::Client::new(credentials),
                        account,
                    )?;
                    let releases = bound
                        .package_releases(&repository)
                        .map_err(|error| error.to_string())?;
                    Ok::<_, String>(release_choices(releases, prereleases))
                })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                this.sync_account(cx);
                if !this.epoch.accepts(ticket) {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(choices) => {
                        if choices.is_empty() {
                            this.error =
                                Some("No eligible published release has ZIP assets.".into());
                        }
                        if let Some(editor) = &mut this.editor {
                            editor.releases = choice(
                                choices.iter().map(|(label, _, _)| label.clone()).collect(),
                                window,
                                cx,
                            );
                            editor.release_ids = choices;
                        }
                        this.watch_release(cx);
                        this.capture_form(cx);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn watch_release(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        self.release_subscription = Some(cx.subscribe(
            &editor.releases,
            |this,
             _,
             event: &gpui_kit::component::select::SelectEvent<
                gpui_kit::component::select::SearchableVec<String>,
            >,
             cx| {
                if matches!(event, gpui_kit::component::select::SelectEvent::Confirm(_)) {
                    this.editor_changed(cx);
                }
            },
        ));
    }
}

fn release_choices(
    releases: Vec<subspace_framework::client_github::PackageRelease>,
    prereleases: bool,
) -> Vec<(String, u64, String)> {
    releases
        .into_iter()
        .filter(|release| {
            !release.draft && release.published_at.is_some() && (prereleases || !release.prerelease)
        })
        .flat_map(|release| {
            release
                .assets
                .into_iter()
                .filter(|asset| asset.name.ends_with(".zip"))
                .map(move |asset| {
                    (
                        format!("{} · {}", release.tag_name, asset.name),
                        release.id,
                        asset.name,
                    )
                })
        })
        .collect()
}
