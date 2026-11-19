//! Registration-driven settings and feature pages.

#[cfg(test)]
mod experiment_tests;
pub(super) mod features;
pub(super) mod general;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, rems,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::settings::SettingsSection;
use subspace_dashboard::toggle::toggle_row;

use super::background_updates::background_update_notes;
use super::{Settings, note, page_frame, text};
use crate::settings::form::SettingsPage;

/// The width each board-control column asks for. Two columns fit side by
/// side once the page is wider than two of these plus the gap between.
const COLUMN_BASIS_REM: f32 = 17.;

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    match &settings.page {
        SettingsPage::Experiments => experiments_page(settings, tokens, cx),
        SettingsPage::Board(id) => board_page(settings, id, tokens, cx),
        SettingsPage::BoardSection { board, path } => {
            section_page(settings, board, path, tokens, cx)
        }
        _ => v_flex().into_any_element(),
    }
}

fn board_page(
    settings: &Settings,
    id: &str,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let Some(board) = settings.boards.iter().find(|board| board.id == id) else {
        return v_flex().into_any_element();
    };
    let experiments = features::board_experiments(board.settings);
    let locked = settings.form.board_locked(board.id);
    let own_fields = settings
        .board_field_rows(board.id, locked, tokens, cx)
        .unwrap_or_else(|| {
            super::connection::render(
                settings,
                board.connection_settings().fields(),
                locked,
                tokens,
                cx,
            )
        });
    let body = v_flex()
        .gap_3()
        .child(board_controls(settings, board.id, locked, tokens, cx))
        .child(own_fields)
        .children(features::stable_feature_rows(
            settings,
            board.settings.features,
            locked,
            tokens,
            cx,
        ))
        .children(
            settings
                .board_editors
                .iter()
                .filter(|draft| draft.id == id)
                .map(|draft| draft.editor.view()),
        )
        .children(features::experiment_footer_items(
            settings,
            &experiments,
            locked,
            tokens,
            cx,
        ));
    page_frame(
        board_description(board.settings.description),
        body,
        tokens,
        cx,
    )
    .into_any_element()
}

/// The board's own description, or a generic line for a board without one.
fn board_description(description: &'static str) -> &'static str {
    if description.is_empty() {
        "Board availability, connection settings and registered features."
    } else {
        description
    }
}

fn section_page(
    settings: &Settings,
    board: &str,
    path: &[usize],
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let Some(section) = settings
        .boards
        .iter()
        .find(|registration| registration.id == board)
        .and_then(|registration| section_at(registration.settings.sections, path))
    else {
        return v_flex().into_any_element();
    };
    let locked = settings.form.board_locked(board);
    let body = v_flex()
        .gap_3()
        .when(locked, |body| body.child(board_off_note(tokens)))
        .children(features::stable_feature_rows(
            settings,
            section.features,
            locked,
            tokens,
            cx,
        ))
        .children(features::experiment_footer(
            settings,
            section.features,
            locked,
            tokens,
            cx,
        ));
    page_frame("Options for this Space.", body, tokens, cx).into_any_element()
}

pub(super) fn section_at(
    mut sections: &'static [SettingsSection],
    path: &[usize],
) -> Option<&'static SettingsSection> {
    let mut selected = None;
    for index in path {
        let section = sections.get(*index)?;
        selected = Some(section);
        sections = section.children;
    }
    selected
}

fn experiments_page(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let body = v_flex()
        .gap_4()
        .children(features::app_experiments(settings, tokens, cx));
    page_frame(
        "Opt in to unfinished features. Changes apply after Save and restart.",
        body,
        tokens,
        cx,
    )
    .into_any_element()
}

/// Enabled on its own row, then Background updates and Update interval, in
/// two equal columns so the switches line up. The columns stack when the
/// page is narrow, and each help line stays under the row it explains.
fn board_controls(
    settings: &Settings,
    id: &'static str,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> impl IntoElement {
    let background = settings.background_update_controls(&SettingsPage::Board(id), locked, cx);
    v_flex()
        .gap_1()
        .child(columns(
            "settings-enabled-row",
            [
                enabled_switch(settings, id, cx).into_any_element(),
                div().into_any_element(),
            ],
        ))
        .child(note(
            tokens,
            "Save applies this Space choice without a restart.",
        ))
        .when(locked, |controls| controls.child(board_off_note(tokens)))
        .when_some(background, |controls, cells| {
            controls
                .child(
                    div()
                        .pt_2()
                        .child(columns("settings-background-row", cells)),
                )
                .children(background_update_notes(tokens))
        })
}

/// Why a turned-off board's settings cannot be changed.
fn board_off_note(tokens: &Tokens) -> impl IntoElement {
    note(tokens, text::BOARD_OFF)
        .id("settings-board-off")
        .test_support()
}

/// Two equal columns, each with its label on the left and its control on
/// the right, that stack into one when the page is too narrow for both.
fn columns(id: &'static str, cells: [AnyElement; 2]) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .w_full()
        .flex_wrap()
        .items_center()
        .gap_x_6()
        .children(cells.into_iter().enumerate().map(|(index, cell)| {
            div()
                .id(index)
                .test_support()
                .flex_1()
                .flex_basis(rems(COLUMN_BASIS_REM))
                .child(cell)
        }))
}

fn enabled_switch(settings: &Settings, id: &str, cx: &mut Context<Settings>) -> impl IntoElement {
    let id = id.to_owned();
    toggle_row(
        text::ENABLED,
        subspace_dashboard::toggle::switch(
            gpui_kit::SharedString::from(format!("settings-{id}-enabled")),
            cx,
        )
        .checked(settings.form.board_enabled(&id))
        .on_click(cx.listener(move |settings, enabled: &bool, _, cx| {
            settings.form.set_board_enabled(&id, *enabled);
            for draft in settings.board_editors.iter().filter(|draft| draft.id == id) {
                draft.editor.set_enabled(*enabled, cx);
            }
            cx.notify();
        })),
    )
    .items_center()
}

#[cfg(test)]
mod tests {
    use super::{features, section_at};
    use subspace_core::features::{FeatureSpec, FeatureStage};
    use subspace_dashboard::settings::{BoardSettings, SettingsSection};
    use subspace_lucide::IconName;

    const LEAF: SettingsSection = SettingsSection {
        id: "leaf",
        title: "Leaf",
        icon: IconName::Settings,
        children: &[],
        features: &[],
    };
    const CHILDREN: &[SettingsSection] = &[LEAF];
    const ROOT: &[SettingsSection] = &[SettingsSection {
        id: "root",
        title: "Root",
        icon: IconName::Settings,
        children: CHILDREN,
        features: &[],
    }];
    const STABLE: FeatureSpec = FeatureSpec {
        id: "board.future.stable",
        title: "Stable",
        description: "Stable fixture.",
        stage: FeatureStage::Stable,
        default_enabled: true,
        toggleable: true,
    };
    const REMOVED: FeatureSpec = FeatureSpec {
        id: "board.future.removed",
        title: "Removed",
        description: "Removed fixture.",
        stage: FeatureStage::Removed,
        default_enabled: false,
        toggleable: false,
    };
    const FEATURES: &[FeatureSpec] = &[STABLE, REMOVED];
    const ROOT_EXPERIMENT: FeatureSpec = FeatureSpec {
        id: "board.future.root-preview",
        title: "Root preview",
        description: "Root fixture experiment.",
        stage: FeatureStage::Experimental,
        default_enabled: false,
        toggleable: true,
    };
    const NESTED_EXPERIMENT: FeatureSpec = FeatureSpec {
        id: "board.future.nested-preview",
        title: "Nested preview",
        description: "Nested fixture experiment.",
        stage: FeatureStage::Experimental,
        default_enabled: false,
        toggleable: true,
    };
    const EXPERIMENT_SECTIONS: &[SettingsSection] = &[SettingsSection {
        id: "preview",
        title: "Preview",
        icon: IconName::Settings,
        children: &[],
        features: &[NESTED_EXPERIMENT],
    }];

    #[test]
    fn arbitrary_nested_sections_resolve_by_registration_path() {
        assert_eq!(
            section_at(ROOT, &[0]).map(|section| section.id),
            Some("root")
        );
        assert_eq!(
            section_at(ROOT, &[0, 0]).map(|section| section.id),
            Some("leaf")
        );
        assert!(section_at(ROOT, &[1]).is_none());
    }

    #[test]
    fn graduated_features_move_to_regular_settings_and_removed_features_stay_hidden() {
        assert_eq!(
            features::features_at_stage(FEATURES, FeatureStage::Stable),
            [&STABLE]
        );
        assert!(features::features_at_stage(FEATURES, FeatureStage::Experimental).is_empty());
    }

    #[test]
    fn board_page_collects_all_nested_experiments() {
        let settings = BoardSettings {
            icon: None,
            sections: EXPERIMENT_SECTIONS,
            features: &[ROOT_EXPERIMENT],
            notifications: false,
            notification_defaults: Default::default(),
            background_updates: false,
            store_by_default: false,
            live_features: &[],
            description: "",
        };
        let ids = features::board_experiments(settings)
            .into_iter()
            .map(|feature| feature.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            ["board.future.root-preview", "board.future.nested-preview"]
        );
    }
}
