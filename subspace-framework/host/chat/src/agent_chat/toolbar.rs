//! The composer toolbar: the icon buttons in the chat input's bottom addon,
//! each opening a GPUI Kit menu, plus the usage gauge and the Send or Stop
//! control.
//!
//! The toolbar renders a plain view model, [`ToolbarModel`], that the panel
//! fills from the agent contract, and reports choices as [`ToolbarEvent`].
//! It holds no agent types except the plain [`Usage`] counters, so the rest
//! of the contract can change without touching it.

pub mod labels;
mod lists;
mod menus;
mod native_menu;
mod overflow;
#[cfg(test)]
mod runtime_tests;
mod usage;

use crate::anchored::Anchor;
use crate::prefs;
use gpui_kit::component::command::CommandState;
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, SharedString, Window};
use subspace_agent::Usage;
use subspace_dashboard::{menu::MenuAnchors, overflow_toolbar::OverflowToolbar};

/// The SDK session modes Subspace offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatModeOption {
    #[default]
    Interactive,
    Plan,
    Autopilot,
}

impl ChatModeOption {
    pub const ALL: [Self; 3] = [Self::Interactive, Self::Plan, Self::Autopilot];
}

/// A model and the reasoning efforts it supports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelOption {
    pub id: SharedString,
    pub label: SharedString,
    pub efforts: Vec<SharedString>,
}

/// An agent, or any other plain identifier and label pair.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamedOption {
    pub id: SharedString,
    pub label: SharedString,
}

/// Everything the toolbar shows. The panel owns it and replaces it whenever
/// the runtime confirms a change.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarModel {
    pub copilot_controls: bool,
    pub show_ai_units: bool,
    pub models: Vec<ModelOption>,
    pub model: Option<SharedString>,
    pub effort: Option<SharedString>,
    pub mode: ChatModeOption,
    pub approve_all: bool,
    pub agents: Vec<NamedOption>,
    pub agent: Option<SharedString>,
    pub auto_context: bool,
    pub show_tools: bool,
    pub show_thinking: bool,
    pub busy: bool,
    pub ready: bool,
    /// A selection the `+` menu can add, if the active dashboard has one.
    pub has_selection: bool,
    /// What the SDK reported for the last request.
    pub usage: Usage,
}

/// What the user chose in the toolbar. The panel turns these into commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarEvent {
    AttachFile,
    AddSelection,
    ToggleAutoContext,
    SetMode(ChatModeOption),
    SetApproveAll(bool),
    /// The panel keeps the reasoning effort if the new model supports it.
    SetModel {
        model: SharedString,
    },
    /// `None` is the model's own default.
    SetEffort(Option<SharedString>),
    SetAgent(SharedString),
    ToggleTools,
    ToggleThinking,
    Send,
    Stop,
}

pub struct ChatToolbar {
    model: ToolbarModel,
    /// Interaction state of the searchable model list.
    model_search: Entity<CommandState>,
    model_anchor: Entity<Anchor>,
    usage_anchor: Entity<Anchor>,
    anchors: MenuAnchors,
    overflow: Option<Entity<OverflowToolbar>>,
    /// Starred model ids, listed first in the model list.
    favourites: Vec<String>,
}

impl EventEmitter<ToolbarEvent> for ChatToolbar {}

/// State file key of the starred model ids.
const FAVOURITES_KEY: &str = "chat.favourite-models";

impl ChatToolbar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let anchors = MenuAnchors::default();
        let model_anchor = anchors.anchor("chat-model", cx);
        let usage_anchor = anchors.anchor("chat-usage", cx);
        Self {
            favourites: prefs::get_list(cx, FAVOURITES_KEY),
            model: ToolbarModel::default(),
            model_search: cx.new(|cx| CommandState::new(window, cx)),
            model_anchor,
            usage_anchor,
            anchors,
            overflow: None,
        }
    }

    pub fn set_model(&mut self, model: ToolbarModel, cx: &mut Context<Self>) {
        if self.model != model {
            self.model = model;
            cx.notify();
        }
    }

    pub fn model(&self) -> &ToolbarModel {
        &self.model
    }

    /// Star or unstar a model, and remember the choice.
    pub fn toggle_favourite(&mut self, id: &str, cx: &mut Context<Self>) {
        labels::toggle_favourite(&mut self.favourites, id);
        prefs::set_list(cx, FAVOURITES_KEY, &self.favourites);
        cx.notify();
    }
}
