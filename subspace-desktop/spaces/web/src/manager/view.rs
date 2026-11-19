use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::{Tokens, anchored, toggle::toggle_row};
use subspace_framework::lucide::IconName;

use super::Manager;
use super::model::SourceKind;

mod collection;
mod editor;

impl Render for Manager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex().id("web-boards-settings").min_w_0().gap_4()
            .child(note("Install websites and ready-built browser apps. Confirmed board installs, renames and removals apply immediately. Settings Save and Cancel do not undo those board changes.", &tokens))
            .child(self.commands(cx))
            .child(self.collection(&tokens, cx))
            .children(self.editor.as_ref().map(|_| self.form(&tokens, cx)))
            .children((self.pending.is_some() && self.editor.is_some()).then(|| self.preview(&tokens, cx)))
            .children(self.busy.then(|| note("Retrieving and validating source… Installed boards are unchanged.", &tokens)))
            .children(self.message.as_ref().map(|message| note(message, &tokens)))
            .children(self.error.as_ref().map(|error| div().text_sm().text_color(tokens.red).child(error.clone())))
    }
}

impl Manager {
    fn commands(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .flex_wrap()
            .child(
                action_button(
                    "web-add",
                    Intent::Neutral,
                    Some(IconName::Plus),
                    "Add board…",
                )
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
            )
            .child(
                action_button(
                    "web-weather",
                    Intent::Neutral,
                    Some(IconName::CloudSun),
                    "Install Weather example…",
                )
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.weather(window, cx))),
            )
            .children(
                (self.busy
                    || self.editor.is_some()
                    || self.pending.is_some()
                    || self.offer.is_some())
                .then(|| {
                    action_button(
                        "web-cancel",
                        Intent::Quiet,
                        Some(IconName::X),
                        "Cancel import",
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx)))
                }),
            )
    }

    fn collection(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        v_flex().gap_3()
            .children(self.installed.is_empty().then(|| note("No web Spaces installed. Add a website, choose a built folder or ZIP, or install from one of your GitHub organizations.", tokens)))
            .children(self.installed.iter().map(|board| self.board_row(board, tokens, cx)))
    }

    fn form(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = &self.editor else {
            return div().into_any_element();
        };
        let body = v_flex()
            .gap_3()
            .min_w_0()
            .child(div().text_base().child(if editor.replacing {
                "Edit board"
            } else {
                "New board"
            }))
            .child(field(
                "web-id",
                "Board ID",
                &editor.id,
                self.busy || editor.replacing,
                tokens,
            ))
            .child(field("web-name", "Name", &editor.title, self.busy, tokens));
        body.children(editor.rename_only.then(|| Self::rename_actions(cx)))
            .children((!editor.rename_only).then(|| self.source_fields(editor, tokens, cx)))
            .into_any_element()
    }

    fn source_fields(
        &self,
        editor: &super::editor::Editor,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let kind = editor.source_kind(cx);
        v_flex().gap_3().child(v_flex().gap_1().child(note("Source", tokens))
                .child(anchored::select(&editor.kind).accessibility_label("Source type").disabled(self.busy)))
            .child(field("web-location", if kind.is_github() { "GitHub repository: owner/name or root URL" } else { "URL or absolute path" }, &editor.location, self.busy, tokens))
            .children(matches!(kind, SourceKind::Folder | SourceKind::Zip).then(|| action_button("web-browse-path", Intent::Neutral, Some(IconName::FolderOpen), "Choose…")
                .disabled(self.busy).on_click(cx.listener(|this, _, window, cx| this.choose_path(window, cx)))))
            .children((kind == SourceKind::Repository).then(|| self.repository_fields(tokens)))
            .children((kind == SourceKind::Release).then(|| self.release_fields(tokens, cx)))
            .children(kind.is_github().then(|| note("Only organizations of the current GitHub account are eligible. Repository read access is checked separately. Imported files are never built or run on the host.", tokens)))
            .child(action_button("web-validate", Intent::Neutral, Some(IconName::ShieldCheck), "Validate source")
                .disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.validate(cx))))
    }

    fn repository_fields(&self, tokens: &Tokens) -> Div {
        let mut body = v_flex().gap_3();
        if let Some(editor) = &self.editor {
            body = body.child(field("web-ref", "Ref: blank for default branch", &editor.reference, self.busy, tokens))
                .child(note("Enter a branch name, tag:v1.0 or commit:<full SHA>. Pinned tags and commits do not track a branch.", tokens))
                .child(field("web-directory", "Board directory: blank for repository root", &editor.board_path, self.busy, tokens))
                .child(note("The selected directory must already contain subspace-board.json, index.html and built assets.", tokens));
        }
        body
    }

    fn release_fields(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut body = v_flex().gap_3();
        if let Some(editor) = &self.editor {
            body = body
                .child(toggle_row(
                    "Include prereleases",
                    subspace_framework::dashboard::toggle::switch("web-prereleases", cx)
                        .checked(editor.prereleases)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                            if let Some(editor) = &mut this.editor {
                                editor.prereleases = *enabled;
                                editor.release_ids.clear();
                            }
                            this.pending = None;
                            this.epoch.invalidate();
                            cx.notify();
                        })),
                ))
                .child(
                    action_button(
                        "web-releases",
                        Intent::Neutral,
                        Some(IconName::Download),
                        "Load release ZIPs",
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load_releases(window, cx))),
                )
                .child(
                    anchored::select(&editor.releases)
                        .accessibility_label("Release and ZIP asset")
                        .disabled(self.busy),
                )
                .child(note(
                    "Updates match this exact ZIP asset name in eligible newer releases.",
                    tokens,
                ));
        }
        body
    }

    fn preview(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut body = v_flex()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(tokens.line);
        if let Some(pending) = &self.pending {
            body = body
                .child(format!(
                    "Ready to {}: {}",
                    if pending.replacing {
                        "replace"
                    } else {
                        "install"
                    },
                    pending.title
                ))
                .children(
                    pending
                        .prepared
                        .name()
                        .map(|name| note(&format!("Package: {name}"), tokens)),
                )
                .child(note(&pending.prepared.metadata().label(), tokens))
                .children(
                    pending
                        .prepared
                        .version()
                        .map(|version| note(&format!("Package version {version}"), tokens)),
                )
                .child(note(
                    &format!(
                        "Requested native permissions: {}. Nothing is granted automatically.",
                        permission_names(pending.prepared.permissions())
                    ),
                    tokens,
                ))
                .children(
                    pending
                        .prepared
                        .warnings()
                        .iter()
                        .map(|warning| note(warning, tokens)),
                )
                .child(
                    action_button(
                        "web-confirm-install",
                        Intent::Neutral,
                        Some(IconName::Check),
                        if pending.replacing {
                            "Replace now"
                        } else {
                            "Install now"
                        },
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.commit(window, cx))),
                );
        }
        body
    }

    fn update_offer(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut body = v_flex().gap_2();
        if let Some(offer) = &self.offer {
            body = body.child(format!("Update available for {}", offer.title))
                .child(note(&offer.offer.label(), tokens))
                .child(note("The installed copy is unchanged. Download and validate before confirming replacement.", tokens))
                .child(action_button("web-prepare-update", Intent::Neutral, Some(IconName::Download), "Download update")
                    .on_click(cx.listener(|this, _, _, cx| this.prepare_update(cx))));
        }
        body
    }

    fn removal(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let title = self
            .remove
            .as_ref()
            .and_then(|id| self.installed.iter().find(|board| &board.id == id))
            .map(|board| board.title.as_str())
            .unwrap_or_default();
        v_flex().gap_2().child(format!("Remove {title}?"))
            .child(note("This unregisters the board immediately. Original app files and browsing data are not deleted.", tokens))
            .child(h_flex().gap_2()
                .child(action_button("web-remove-cancel", Intent::Neutral, Some(IconName::X), "Keep board")
                    .on_click(cx.listener(|this, _, _, cx| { this.remove = None; cx.notify(); })))
                .child(action_button("web-remove-confirm", Intent::Reject, Some(IconName::Trash), "Remove board")
                    .on_click(cx.listener(|this, _, window, cx| this.remove_board(window, cx)))))
    }
}

fn note(text: &str, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .text_sm()
        .text_color(tokens.muted)
        .child(text.to_owned())
}

fn field(
    id: &'static str,
    label: &str,
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
    disabled: bool,
    tokens: &Tokens,
) -> Div {
    v_flex().gap_1().child(note(label, tokens)).child(
        fields::input(input, InputRole::Configuration, label.to_owned())
            .id(id)
            .disabled(disabled),
    )
}

fn permission_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}
