use gpui_kit::component::{Disableable as _, Selectable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::panel::IconName;

use super::{Choice, ProjectSettings, Row, SettingsEvent, all_approved};
use crate::private_settings::ImportPreview;

impl ProjectSettings {
    fn row(&self, name: &str, row: &Row, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().child(name.to_owned()))
            .child(div().text_sm().child(self.row_status(name)))
            .child(self.row_actions(name, row, cx))
            .when(row.choice == Choice::Replace, |element| {
                element.child(
                    fields::input(
                        &row.input,
                        InputRole::Configuration,
                        format!("Replacement value for {name}"),
                    )
                    .disabled(self.busy),
                )
            })
    }

    fn row_status(&self, name: &str) -> String {
        let saved = self
            .metadata
            .as_ref()
            .is_some_and(|m| m.names().iter().any(|n| n == name));
        let selected = self.declared.iter().any(|n| n == name);
        format!(
            "{} · {}",
            if selected {
                "Selected for project"
            } else {
                "Not selected for project"
            },
            if saved { "Saved" } else { "Not saved" }
        )
    }

    fn row_actions(&self, name: &str, row: &Row, cx: &mut Context<Self>) -> Div {
        let mut controls = div().flex().gap_2();
        for (choice, label, icon) in [
            (Choice::Keep, "Keep", IconName::Check),
            (Choice::Replace, "Replace", IconName::Pencil),
            (Choice::Clear, "Clear", IconName::Trash),
        ] {
            let key = name.to_owned();
            let selected_choice = choice == row.choice;
            controls = controls.child(
                action_button(
                    format!("private-{name}-{label}"),
                    Intent::Neutral,
                    Some(icon),
                    label,
                )
                .accessibility_label(format!("{label} {name}"))
                .selected(selected_choice)
                .toggled(selected_choice)
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(row) = this.rows.get_mut(&key) {
                        row.choice = choice;
                    }
                    cx.notify();
                })),
            );
        }
        controls
    }

    fn import_view(&self, cx: &mut Context<Self>) -> Div {
        let area = div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Import dotenv file")
            .child(
                fields::input(&self.path, InputRole::Configuration, "Dotenv file path")
                    .disabled(self.busy),
            )
            .child(
                action_button(
                    "preview-dotenv",
                    Intent::Neutral,
                    Some(IconName::Upload),
                    "Preview import",
                )
                .disabled(self.busy || self.metadata.is_none())
                .on_click(cx.listener(|this, _, window, cx| this.preview_import(window, cx))),
            );
        match &self.preview {
            Some(preview) => self.import_preview(area, preview, cx),
            None => area,
        }
    }

    fn import_preview(
        &self,
        mut area: Div,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> Div {
        let pending = self.rows.values().any(|row| row.choice != Choice::Keep);
        area = area.child(format!("Names: {}", preview.names().join(", ")));
        if !preview.conflicts().is_empty() {
            area = self.import_conflicts(area, preview, cx);
        }
        if pending {
            area = area.child("Save or discard pending edits before importing.");
        }
        area = area.child(
            action_button(
                "apply-dotenv",
                Intent::Neutral,
                Some(IconName::Upload),
                "Import values",
            )
            .disabled(self.busy || pending || !all_approved(preview.conflicts(), &self.approved))
            .on_click(cx.listener(|this, _, window, cx| this.apply_import(window, cx))),
        );
        area
    }

    fn import_conflicts(&self, area: Div, preview: &ImportPreview, cx: &mut Context<Self>) -> Div {
        let area = area.child(format!(
            "Existing values to replace: {}",
            preview.conflicts().join(", ")
        ));
        let approved = all_approved(preview.conflicts(), &self.approved);
        area.child(
            action_button(
                "approve-import-conflicts",
                Intent::Neutral,
                Some(IconName::Check),
                if approved {
                    "All replacements approved"
                } else {
                    "Approve replacing all listed values"
                },
            )
            .selected(approved)
            .toggled(approved)
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(preview) = &this.preview {
                    if all_approved(preview.conflicts(), &this.approved) {
                        this.approved.clear();
                    } else {
                        this.approved = preview.conflicts().iter().cloned().collect();
                    }
                }
                cx.notify();
            })),
        )
    }
}

impl Render for ProjectSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .rows
            .iter()
            .map(|(name, row)| self.row(name, row, cx))
            .collect::<Vec<_>>();
        let import = self.import_view(cx);
        div().id("private-settings").size_full().overflow_y_scroll().p_4().flex().flex_col().gap_4()
            .child("Private environment settings")
            .child("Values are stored in an application-private file protected by OS permissions, not a keychain. A delegated runtime can read and transmit selected values.")
            .when(self.busy, |element| element.child("Working…"))
            .when(self.metadata.is_some() && rows.is_empty(), |element| element.child("No environment names declared or saved."))
            .children(rows)
            .child(import)
            .children(self.error.map(|error| div().child(error)))
            .child(div().flex().gap_2()
                .child(action_button("save-private-settings", Intent::Primary, Some(IconName::Save), "Save")
                    .disabled(self.busy || self.metadata.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                .child(action_button("reload-private-settings", Intent::Neutral, Some(IconName::RefreshCw), "Reload")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))) )
                .child(action_button("close-private-settings", Intent::Neutral, Some(IconName::X), "Close")
                    .disabled(self.busy)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)))))
    }
}
