//! Compare retained editor values, never live preferences or stored credentials.

use std::collections::BTreeMap;

use gpui_kit::App;
use gpui_kit::component::select::{SelectDelegate, SelectItem};
use subspace_core::features::FeatureSpec;
use subspace_dashboard::settings::SettingsSection;

use super::toolbar::ToolbarDraft;
use super::{APP_FEATURES, Settings, board_fields, form::Picker};
use crate::notifications::NotificationPreferences;
use board_fields::Control;

#[derive(Clone, PartialEq)]
pub(super) struct DraftSnapshot {
    services: ServiceDraft,
    appearance: AppearanceDraft,
    layout: LayoutDraft,
    preferences: PreferenceDraft,
    notifications: NotificationPreferences,
    sources: Vec<(&'static str, subspace_dashboard::notifications::Preferences)>,
    background: Vec<(&'static str, subspace_dashboard::refresh::Preferences)>,
    board_fields: BTreeMap<&'static str, BoardDraft>,
    board_editors: BTreeMap<&'static str, serde_json::Value>,
}

#[derive(Clone, PartialEq)]
struct BoardDraft {
    fields: BTreeMap<String, BoardFieldDraft>,
    permissions: BTreeMap<String, bool>,
}

/// Only retained editor values, including invalid text and newly entered secrets.
/// Saved secret presence and mutable save baselines do not affect draft identity.
#[derive(Clone, PartialEq)]
enum BoardFieldDraft {
    Text(String),
    Toggle(bool),
    Choice(Option<String>),
    Secret { text: String, clear: bool },
}

// These are only newly entered input values. No saved secret enters the editor.
#[derive(Clone, PartialEq)]
struct ServiceDraft {
    repos: Vec<String>,
    credentials: [(String, bool); 1],
    boards: BTreeMap<String, bool>,
    features: BTreeMap<&'static str, bool>,
    engine: subspace_core::config::ChatEngine,
    acp_agent: subspace_core::config::AcpAgent,
    account: Option<String>,
    mcp: super::mcp_draft::McpFields,
}

#[derive(Clone, PartialEq)]
struct AppearanceDraft {
    theme: Option<String>,
    fonts: [Option<String>; 3],
    sizes: Vec<Option<f32>>,
    icon_button_size: Option<String>,
}

#[derive(Clone, PartialEq)]
struct LayoutDraft {
    boards: Vec<&'static str>,
    navigation: Option<String>,
    collapsed: bool,
    terminal: Option<super::terminal::TerminalSnapshot>,
    toolbar: ToolbarDraft,
}

#[derive(Clone, PartialEq)]
struct PreferenceDraft {
    confirm_quit: bool,
    check_updates: bool,
    auto_refresh: bool,
    approve_all: bool,
    developer_mode: bool,
}

impl DraftSnapshot {
    pub(super) fn reconcile_membership(&mut self, current: &Self) {
        self.layout
            .boards
            .retain(|id| current.layout.boards.contains(id));
        for id in &current.layout.boards {
            if !self.layout.boards.contains(id) {
                self.layout.boards.push(id);
            }
        }
        reconcile_map(&mut self.services.boards, &current.services.boards);
        reconcile_map(&mut self.board_editors, &current.board_editors);
        reconcile_map(&mut self.services.features, &current.services.features);
        self.sources
            .retain(|(id, _)| current.sources.iter().any(|(next, _)| id == next));
        for value in &current.sources {
            if !self.sources.iter().any(|(id, _)| id == &value.0) {
                self.sources.push(*value);
            }
        }
        self.background
            .retain(|(id, _)| current.background.iter().any(|(next, _)| id == next));
        for value in &current.background {
            if !self.background.iter().any(|(id, _)| id == &value.0) {
                self.background.push(*value);
            }
        }
        self.board_fields
            .retain(|id, _| current.board_fields.contains_key(id));
        for (id, value) in &current.board_fields {
            let draft = self.board_fields.entry(id).or_insert_with(|| value.clone());
            reconcile_map(&mut draft.fields, &value.fields);
            reconcile_map(&mut draft.permissions, &value.permissions);
        }
    }
}

fn reconcile_map<K: Ord + Clone, V: Clone>(before: &mut BTreeMap<K, V>, current: &BTreeMap<K, V>) {
    before.retain(|key, _| current.contains_key(key));
    for (key, value) in current {
        before.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

impl Settings {
    /// Net editor changes since opening or the last successful complete Save.
    /// Previewing or external preference changes do not replace this baseline.
    pub(crate) fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.initial_draft.as_ref() != Some(&self.draft_snapshot(cx))
            || self
                .partial_draft
                .as_ref()
                .is_some_and(|draft| Some(draft) != self.initial_draft.as_ref())
    }

    pub(super) fn draft_snapshot(&self, cx: &App) -> DraftSnapshot {
        DraftSnapshot {
            services: self.service_draft(cx),
            appearance: AppearanceDraft {
                theme: selection(&self.themes, cx),
                fonts: self.fonts.each_ref().map(|font| selection(font, cx)),
                sizes: super::sizes::values(&self.font_sizes, cx),
                icon_button_size: selection(&self.icon_buttons.choice, cx),
            },
            layout: LayoutDraft {
                boards: self.boards.iter().map(|board| board.id).collect(),
                navigation: selection(&self.board_navigation.mode, cx),
                collapsed: self.board_navigation.collapsed,
                terminal: self.terminal.as_ref().map(|terminal| terminal.snapshot(cx)),
                toolbar: self.toolbar,
            },
            preferences: PreferenceDraft {
                confirm_quit: self.confirm_quit,
                check_updates: self.check_updates,
                auto_refresh: self.auto_refresh,
                approve_all: self.approve_all_default,
                developer_mode: self.developer_mode,
            },
            notifications: self.notifications,
            sources: self
                .sources
                .iter()
                .map(|source| (source.id, source.preferences(cx)))
                .collect(),
            background: self
                .background_updates
                .iter()
                .map(|draft| (draft.id, draft.preferences(cx)))
                .collect(),
            board_fields: board_drafts(&self.board_fields, cx),
            board_editors: super::board_editors::snapshots(&self.board_editors, cx),
        }
    }

    fn service_draft(&self, cx: &App) -> ServiceDraft {
        ServiceDraft {
            repos: self.form.repos.read(cx).favourites().to_vec(),
            credentials: self
                .form
                .tokens
                .each_ref()
                .map(|token| (token.input.read(cx).value().to_string(), token.clear)),
            boards: self.form.board_draft(),
            features: self.feature_draft(),
            engine: self.selected_chat_engine(cx),
            acp_agent: self.selected_acp_agent(cx),
            account: selection(&self.account, cx),
            mcp: self.mcp.fields(cx),
        }
    }

    fn feature_draft(&self) -> BTreeMap<&'static str, bool> {
        let mut features: Vec<_> = APP_FEATURES.iter().collect();
        for board in &self.boards {
            features.extend(board.settings.features);
            section_features(board.settings.sections, &mut features);
        }
        features
            .into_iter()
            .map(|feature| (feature.id, self.form.feature_enabled(feature)))
            .collect()
    }
}

fn selection<D>(choice: &Picker<D>, cx: &App) -> Option<String>
where
    D: SelectDelegate + 'static,
    D::Item: SelectItem<Value = String>,
{
    choice.read(cx).selected_value().cloned()
}

fn board_drafts(drafts: &[board_fields::Draft], cx: &App) -> BTreeMap<&'static str, BoardDraft> {
    drafts
        .iter()
        .map(|draft| {
            let fields = draft
                .fields
                .iter()
                .map(|field| {
                    (
                        field.spec.key.clone(),
                        board_field_draft(&field.control, cx),
                    )
                })
                .collect();
            let permissions = draft
                .permissions
                .iter()
                .map(|grant| (grant.key.clone(), grant.on))
                .collect();
            (
                draft.id,
                BoardDraft {
                    fields,
                    permissions,
                },
            )
        })
        .collect()
}

fn board_field_draft(control: &Control, cx: &App) -> BoardFieldDraft {
    match control {
        Control::Text { input, .. } => BoardFieldDraft::Text(input.read(cx).value().to_string()),
        Control::Toggle { on, .. } => BoardFieldDraft::Toggle(*on),
        Control::Choice { choice, .. } => BoardFieldDraft::Choice(selection(choice, cx)),
        Control::Secret { input, clear, .. } => BoardFieldDraft::Secret {
            text: input.read(cx).value().to_string(),
            clear: *clear,
        },
    }
}

fn section_features(
    sections: &'static [SettingsSection],
    features: &mut Vec<&'static FeatureSpec>,
) {
    for section in sections {
        features.extend(section.features);
        section_features(section.children, features);
    }
}
