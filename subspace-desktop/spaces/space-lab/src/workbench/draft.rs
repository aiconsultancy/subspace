//! Draft canvas loading, parameter binding and persistence helpers for
//! [`super::Workbench`]: switching recipe profiles, opening a saved preset,
//! and keeping the bound input widgets in sync with `draft.parameters`.

use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{App, AppContext as _, Context, Entity, Window};

use crate::contracts::{Parameters, Preset};

use super::state::Profile;
use super::{ADDABLE_PARAMETERS, Workbench};

/// Reads the one named field of `Parameters` a control's `parameter` names.
pub(super) fn parameter_value<'a>(parameters: &'a Parameters, name: &str) -> &'a str {
    match name {
        "start" => &parameters.start,
        "end" => &parameters.end,
        "currency" => &parameters.currency,
        "service" => &parameters.service,
        _ => &parameters.group_by,
    }
}

pub(super) fn set_parameter(parameters: &mut Parameters, name: &str, value: String) {
    match name {
        "start" => parameters.start = value,
        "end" => parameters.end = value,
        "currency" => parameters.currency = value,
        "service" => parameters.service = value,
        _ => parameters.group_by = value,
    }
}

pub(super) fn text_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(value.to_owned(), window, cx);
        input
    })
}

pub(super) fn fallback_preset() -> Preset {
    Preset {
        id: "draft".into(),
        title: "Untitled canvas".into(),
        environment: "python-data".into(),
        recipe: "market".into(),
        parameters: Parameters::default(),
        controls: Vec::new(),
        view: "bar".into(),
        snapshot: None,
        revision: 0,
        updated_at: String::new(),
    }
}

impl Workbench {
    pub(super) fn rebuild_param_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs.param_inputs.clear();
        self.inputs.param_selects.clear();
        for control in &self.draft.controls {
            let value = parameter_value(&self.draft.parameters, &control.parameter).to_owned();
            let Some((key, _)) = ADDABLE_PARAMETERS
                .iter()
                .find(|(name, _)| *name == control.parameter)
            else {
                continue;
            };
            if control.kind == "select" {
                let options = control.options.clone();
                let state = cx.new(|cx| {
                    let mut state = SelectState::new(SearchableVec::new(options), None, window, cx)
                        .searchable(true);
                    state.set_selected_value(&value, window, cx);
                    state
                });
                self.inputs.param_selects.insert(*key, state);
                continue;
            }
            let state = cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(value, window, cx);
                input
            });
            self.inputs.param_inputs.insert(*key, state);
        }
    }

    pub(super) fn load_profile(
        &mut self,
        profile: Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.jobs.busy {
            return;
        }
        self.view.profile = profile;
        match self.studio.new_draft(profile.key()) {
            Ok(draft) => {
                self.draft = draft;
                self.last_result = None;
                self.jobs.status = format!("New {} draft", profile.title()).into();
            }
            Err(error) => self.jobs.status = format!("Could not start a draft: {error:#}").into(),
        }
        self.inputs.title_input = text_input(&self.draft.title, window, cx);
        self.recipe_source = self
            .studio
            .recipe_source(&self.draft)
            .unwrap_or_else(|error| error.to_string());
        self.networked = self.studio.recipe_network(&self.draft).unwrap_or(false);
        self.rebuild_param_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn open_preset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.jobs.busy {
            return;
        }
        if let Some(preset) = self.presets.iter().find(|preset| preset.id == id).cloned() {
            self.draft = preset;
            self.last_result =
                self.draft
                    .snapshot
                    .clone()
                    .map(|dataset| crate::contracts::RunResult {
                        dataset,
                        stdout: String::new(),
                        runtime: "saved snapshot".into(),
                        elapsed_ms: 0,
                    });
            self.inputs.title_input = text_input(&self.draft.title, window, cx);
            self.recipe_source = self
                .studio
                .recipe_source(&self.draft)
                .unwrap_or_else(|_| "Static canvas snapshot. No refresh recipe.".into());
            self.networked = self.studio.recipe_network(&self.draft).unwrap_or(false);
            self.rebuild_param_inputs(window, cx);
            self.view.tab = super::state::ViewTab::Canvas;
            self.jobs.status = format!("Opened {}", self.draft.title).into();
            cx.notify();
        }
    }

    pub(super) fn apply_parameter_inputs(&mut self, cx: &App) {
        self.draft.title = self.inputs.title_input.read(cx).value().to_string();
        for (name, state) in &self.inputs.param_inputs {
            let value = state.read(cx).value().to_string();
            set_parameter(&mut self.draft.parameters, name, value);
        }
        for (name, state) in &self.inputs.param_selects {
            if let Some(value) = state.read(cx).selected_value() {
                set_parameter(&mut self.draft.parameters, name, value.clone());
            }
        }
    }

    pub(super) fn add_control(
        &mut self,
        parameter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.jobs.busy {
            return;
        }
        self.apply_parameter_inputs(cx);
        if let Err(error) = self.studio.add_control(&mut self.draft, parameter) {
            self.jobs.status = format!("Could not add control: {error:#}").into();
        }
        self.rebuild_param_inputs(window, cx);
        cx.notify();
    }
}
