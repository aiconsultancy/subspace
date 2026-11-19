//! The service settings form: its inputs, its token fields and the values
//! they produce for `subspace-core`'s settings file.

use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectDelegate, SelectItem, SelectState};
use gpui_kit::{App, AppContext as _, Entity, Window};
use subspace_core::Secret;
use subspace_core::config::Config;
use subspace_core::features::{FeatureSpec, FeatureStage};
use subspace_core::settings::{ServiceSettings, TokenEdit};
use subspace_dashboard::settings::ConnectionToken;

/// A registration-driven settings destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SettingsPage {
    General,
    Account,
    Chat,
    Mcp,
    Appearance,
    Layout,
    Notifications,
    Experiments,
    /// The docked terminal's fonts, for a product with a terminal.
    Terminal,
    #[cfg(debug_assertions)]
    Developer,
    Extension(&'static str),
    Board(&'static str),
    BoardSection {
        board: &'static str,
        path: Vec<usize>,
    },
}

/// The token field of each service, in `ServiceForm::tokens` order.
pub(super) const GITHUB_TOKEN: usize = ConnectionToken::GitHub as usize;

pub(super) struct TokenField {
    pub(super) section: &'static str,
    pub(super) label: &'static str,
    pub(super) input: Entity<InputState>,
    pub(super) clear: bool,
}

impl TokenField {
    pub(super) fn edit(&self, cx: &App) -> TokenEdit {
        token_edit(&self.input.read(cx).value(), self.clear)
    }
}

/// Entered text replaces the saved token. A blank field keeps it unless
/// Clear is selected; a saved token's field accepts text only after Clear.
pub(super) fn token_edit(text: &str, clear: bool) -> TokenEdit {
    if !text.trim().is_empty() {
        TokenEdit::Replace(Secret::new(text.trim()))
    } else if clear {
        TokenEdit::Clear
    } else {
        TokenEdit::Keep
    }
}

pub(super) struct ServiceForm {
    pub(super) repos: Entity<super::repositories::RepositoryList>,
    pub(super) tokens: [TokenField; 1],
    boards: Vec<BoardChoice>,
    original_boards: std::collections::BTreeMap<String, bool>,
    features: std::collections::BTreeMap<String, bool>,
    original_chat_engine: subspace_core::config::ChatEngine,
    original_acp_agent: subspace_core::config::AcpAgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoardChoice {
    id: String,
    enabled: bool,
    default_enabled: bool,
    initial_enabled: bool,
    initial_explicit: Option<bool>,
    changed: bool,
}

/// A select whose items save as their `String` value.
pub(super) type Picker<D> = Entity<SelectState<D>>;
pub(super) type Choice = Picker<SearchableVec<String>>;

pub(super) fn choice(
    items: Vec<String>,
    selected: Option<&str>,
    searchable: bool,
    window: &mut Window,
    cx: &mut App,
) -> Choice {
    picker(SearchableVec::new(items), selected, searchable, window, cx)
}

pub(super) fn picker<D>(
    items: D,
    selected: Option<&str>,
    searchable: bool,
    window: &mut Window,
    cx: &mut App,
) -> Picker<D>
where
    D: SelectDelegate + 'static,
    D::Item: SelectItem<Value = String>,
{
    let selected = selected.map(str::to_owned);
    cx.new(|cx| {
        let mut state = SelectState::new(items, None, window, cx).searchable(searchable);
        if let Some(value) = &selected {
            state.set_selected_value(value, window, cx);
        }
        state
    })
}

impl ServiceForm {
    pub(super) fn reconcile_boards(
        &mut self,
        included: &[(String, bool)],
        values: &ServiceSettings,
    ) {
        self.boards
            .retain(|board| included.iter().any(|(id, _)| id == &board.id));
        for (id, default_enabled) in included {
            if self.boards.iter().any(|board| &board.id == id) {
                continue;
            }
            let initial_explicit = values.boards.get(id).copied();
            let initial_enabled = initial_explicit.unwrap_or(*default_enabled);
            self.boards.push(BoardChoice {
                id: id.clone(),
                enabled: initial_enabled,
                default_enabled: *default_enabled,
                initial_enabled,
                initial_explicit,
                changed: false,
            });
        }
    }
    pub(super) fn new(
        values: &ServiceSettings,
        included_boards: &[(String, bool)],
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self {
            repos: cx.new(|_| super::repositories::RepositoryList::new(values.repos.clone())),
            tokens: [("github", "GitHub token")].map(|(section, label)| TokenField {
                section,
                label,
                input: cx.new(|cx| InputState::new(window, cx).masked(true)),
                clear: false,
            }),
            boards: included_boards
                .iter()
                .map(|(id, default_enabled)| {
                    let initial_explicit = values.boards.get(id).copied();
                    let initial_enabled = initial_explicit.unwrap_or(*default_enabled);
                    BoardChoice {
                        id: id.clone(),
                        enabled: initial_enabled,
                        default_enabled: *default_enabled,
                        initial_enabled,
                        initial_explicit,
                        changed: false,
                    }
                })
                .collect(),
            original_boards: values.boards.clone(),
            features: values.features.clone(),
            original_chat_engine: values.chat_engine,
            original_acp_agent: values.acp_agent,
        }
    }

    /// Save replaces the app-level values, including `mcp`, with its own.
    pub(super) fn values(&self, cx: &App) -> ServiceSettings {
        ServiceSettings {
            mcp: subspace_core::config::McpSettings::default(),
            chat_engine: self.original_chat_engine,
            acp_agent: self.original_acp_agent,
            board_order: Vec::new(),
            auto_refresh: true,
            terminal: serde_json::Map::new(),
            boards: self.board_values(),
            features: self.features.clone(),
            repos: self.repos.read(cx).favourites().to_vec(),
            github_token: self.tokens[GITHUB_TOKEN].edit(cx),
            board_fields: Vec::new(),
            board_files: std::collections::BTreeMap::new(),
        }
    }

    pub(super) fn changed(&self, original: &ServiceSettings, cx: &App) -> bool {
        let values = self.values(cx);
        values.boards != original.boards
            || values.features != original.features
            || values.repos != original.repos
            || self
                .tokens
                .iter()
                .any(|token| !matches!(token.edit(cx), TokenEdit::Keep))
    }

    pub(super) fn board_enabled(&self, id: &str) -> bool {
        self.boards
            .iter()
            .find(|board| board.id == id)
            .is_none_or(|board| board.enabled)
    }

    /// Every board page follows one rule: while its board is turned off in
    /// the draft, its other settings are shown but cannot be changed.
    pub(super) fn board_locked(&self, id: &str) -> bool {
        !self.board_enabled(id)
    }

    pub(super) fn board_draft(&self) -> std::collections::BTreeMap<String, bool> {
        self.boards
            .iter()
            .map(|board| (board.id.clone(), board.enabled))
            .collect()
    }

    pub(super) fn set_board_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(board) = self.boards.iter_mut().find(|board| board.id == id)
            && board.enabled != enabled
        {
            board.enabled = enabled;
            board.changed = true;
        }
    }

    pub(super) fn feature_enabled(&self, feature: &FeatureSpec) -> bool {
        feature_enabled(&self.features, feature)
    }

    pub(super) fn set_feature_enabled(&mut self, feature: &FeatureSpec, enabled: bool) {
        set_feature_enabled(&mut self.features, feature, enabled);
    }

    fn board_values(&self) -> std::collections::BTreeMap<String, bool> {
        merge_board_values(&self.original_boards, &self.boards)
    }
}

fn merge_board_values(
    original: &std::collections::BTreeMap<String, bool>,
    boards: &[BoardChoice],
) -> std::collections::BTreeMap<String, bool> {
    let mut values = original.clone();
    for board in boards {
        if !board.changed {
            continue;
        }
        if board.enabled == board.default_enabled {
            values.remove(&board.id);
        } else {
            values.insert(board.id.clone(), board.enabled);
        }
    }
    values
}

fn feature_enabled(
    features: &std::collections::BTreeMap<String, bool>,
    feature: &FeatureSpec,
) -> bool {
    Config {
        features: features.clone(),
        ..Default::default()
    }
    .feature_enabled(feature)
}

fn set_feature_enabled(
    features: &mut std::collections::BTreeMap<String, bool>,
    feature: &FeatureSpec,
    enabled: bool,
) {
    if feature.toggleable && feature.stage != FeatureStage::Removed {
        features.insert(feature.id.to_owned(), enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BoardChoice, feature_enabled, merge_board_values, set_feature_enabled, token_edit,
    };
    use subspace_core::features::{FeatureSpec, FeatureStage};
    use subspace_core::settings::TokenEdit;

    #[test]
    fn blank_tokens_are_kept_and_clear_is_explicit() {
        assert!(matches!(token_edit("", false), TokenEdit::Keep));
        assert!(matches!(token_edit(" \n", false), TokenEdit::Keep));
        assert!(matches!(token_edit("", true), TokenEdit::Clear));
        for clear in [false, true] {
            let TokenEdit::Replace(secret) = token_edit(" new-token ", clear) else {
                panic!("replacement expected");
            };
            assert_eq!(secret.expose(), "new-token");
        }
    }

    fn feature(stage: FeatureStage, default_enabled: bool, toggleable: bool) -> FeatureSpec {
        FeatureSpec {
            id: "board.future.option",
            title: "Option",
            description: "Fixture option.",
            stage,
            default_enabled,
            toggleable,
        }
    }

    #[test]
    fn feature_drafts_follow_lifecycle_policy_and_keep_one_mirrored_value() {
        let mut draft = std::collections::BTreeMap::new();
        let experimental = feature(FeatureStage::Experimental, false, true);
        assert!(!feature_enabled(&draft, &experimental));
        set_feature_enabled(&mut draft, &experimental, true);
        assert!(feature_enabled(&draft, &experimental));
        set_feature_enabled(&mut draft, &experimental, false);
        assert!(!feature_enabled(&draft, &experimental));

        assert!(!feature_enabled(
            &draft,
            &feature(FeatureStage::Stable, true, true)
        ));
        assert!(feature_enabled(
            &Default::default(),
            &feature(FeatureStage::Stable, true, true)
        ));
        assert!(feature_enabled(
            &draft,
            &feature(FeatureStage::Stable, true, false)
        ));
        assert!(!feature_enabled(
            &draft,
            &feature(FeatureStage::Removed, true, true)
        ));
    }

    #[test]
    fn board_choices_resolve_product_defaults_and_preserve_untouched_overrides() {
        let choice = |id: &str, default_enabled: bool, initial_explicit: Option<bool>| {
            let initial_enabled = initial_explicit.unwrap_or(default_enabled);
            BoardChoice {
                id: id.into(),
                enabled: initial_enabled,
                default_enabled,
                initial_enabled,
                initial_explicit,
                changed: false,
            }
        };
        let untouched_true = choice("release-off", false, Some(true));
        let untouched_false = choice("release-off-explicit", false, Some(false));
        assert!(untouched_true.initial_enabled);
        assert_eq!(untouched_true.initial_explicit, Some(true));
        assert!(!untouched_false.initial_enabled);
        assert_eq!(untouched_false.initial_explicit, Some(false));

        assert!(untouched_true.enabled);
        assert!(!untouched_true.default_enabled);

        let mut enabled_default_off = choice("missing-default-off", false, None);
        enabled_default_off.enabled = true;
        enabled_default_off.changed = true;
        let mut returned_to_default = choice("explicit-on-default-off", false, Some(true));
        returned_to_default.enabled = false;
        returned_to_default.changed = true;
        let saved = merge_board_values(
            &[
                ("release-off".into(), true),
                ("release-off-explicit".into(), false),
                ("unknown".into(), true),
            ]
            .into_iter()
            .collect(),
            &[
                untouched_true,
                untouched_false,
                enabled_default_off,
                returned_to_default,
            ],
        );
        assert!(saved["release-off"]);
        assert!(!saved["release-off-explicit"]);
        assert!(saved["missing-default-off"]);
        assert!(!saved.contains_key("explicit-on-default-off"));
        assert!(saved["unknown"]);
    }
}
