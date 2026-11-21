//! Native presentation of an accepted project state.

mod chart;
mod controls;
mod grid;
mod interaction;
mod render;

use std::collections::HashMap;

use crate::project::{InputValue, LiveState};
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{
    Context, Entity, EventEmitter, FocusHandle, IntoElement, Render, Subscription, Window,
};
use subspace_framework::dashboard::slider::NumericSliderState;

#[derive(Clone, PartialEq)]
struct SelectControl {
    state: Entity<SelectState<SearchableVec<String>>>,
    trigger: FocusHandle,
}

/// User intent for the owner of the authoritative project state.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectViewEvent {
    Input {
        key: String,
        value: InputValue,
        session_id: String,
        expected_revision: u64,
    },
    Action {
        action: String,
        session_id: String,
        expected_revision: u64,
    },
}

/// Retains presentation state, not worker or service ownership.
pub struct ProjectView {
    snapshot: LiveState,
    sliders: HashMap<String, Entity<NumericSliderState>>,
    selects: HashMap<String, SelectControl>,
    grids: HashMap<String, grid::GridControl>,
    subscriptions: Vec<Subscription>,
    generation: u64,
}

impl EventEmitter<ProjectViewEvent> for ProjectView {}

impl ProjectView {
    pub fn new(state: &LiveState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            snapshot: state.clone(),
            sliders: HashMap::new(),
            selects: HashMap::new(),
            grids: HashMap::new(),
            subscriptions: Vec::new(),
            generation: 0,
        };
        view.build_controls(window, cx);
        view.subscribe_controls(cx);
        view.sync_grids(state, window, cx);
        view
    }

    /// Apply an accepted external snapshot without emitting user input.
    pub fn sync(&mut self, state: &LiveState, window: &mut Window, cx: &mut Context<Self>) {
        if self.snapshot.definition() != state.definition() {
            self.grids.clear();
        }
        self.sync_grids(state, window, cx);
        if !interaction::changed(&self.snapshot, state) {
            self.snapshot = state.clone();
            cx.notify();
            return;
        }
        let selects = self.selects.clone();
        let replace = self.snapshot.definition() != state.definition();
        self.subscriptions.clear();
        if replace {
            self.snapshot = state.clone();
            self.sliders.clear();
            self.selects.clear();
            self.build_controls(window, cx);
        } else {
            self.sync_controls(state, window, cx);
            self.snapshot = state.clone();
        }
        self.generation = self.generation.wrapping_add(1);
        self.subscribe_controls(cx);
        if selects != self.selects {
            self.restore_control_focus(selects, window, cx);
        }
        cx.notify();
    }

    fn emit_input(&self, id: &str, key: &str, value: InputValue, cx: &mut Context<Self>) {
        if !self.snapshot.is_enabled(id) || self.snapshot.values().get(key) == Some(&value) {
            return;
        }
        let revision = self.snapshot.input_revision();
        if self
            .snapshot
            .clone()
            .set_value(key, value.clone(), revision)
            .is_ok()
        {
            cx.emit(ProjectViewEvent::Input {
                key: key.into(),
                value,
                session_id: self.snapshot.session_id().to_owned(),
                expected_revision: revision,
            });
        }
    }

    fn emit_action(&self, id: &str, action: &str, cx: &mut Context<Self>) {
        if self.snapshot.is_enabled(id) && self.snapshot.validate_action(action).is_ok() {
            cx.emit(ProjectViewEvent::Action {
                action: action.into(),
                session_id: self.snapshot.session_id().to_owned(),
                expected_revision: self.snapshot.input_revision(),
            });
        }
    }
}

impl Render for ProjectView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_project(cx)
    }
}

#[cfg(test)]
mod tests;
