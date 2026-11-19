use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::{App, AppContext as _, Context, FocusHandle, Focusable as _, Window};
use std::collections::HashMap;

use super::{ProjectView, SelectControl};
use crate::project::{Component, ComponentKind, InputDeclaration, InputValue, LiveState};
use subspace_framework::dashboard::slider::{
    NumericSliderEvent, NumericSliderSpec, NumericSliderState,
};

// Native slider ticks use exactly representable f32 integers.
const MAX_EXACT_TICKS: f64 = 16_777_216.;
// Match the project input validator's allowance for step roundoff.
const STEP_ROUNDOFF_MULTIPLIER: f64 = 4.;
const STEP_ABSOLUTE_TOLERANCE: f64 = 1e-8;
const MINIMUM_SLIDER_STEPS: f64 = 1.;

/// The final slider endpoint must also satisfy the model's step constraint.
pub(super) fn slider_spec(min: f64, max: f64, step: f64) -> Option<NumericSliderSpec> {
    let quotient = (max - min) / step;
    if !quotient.is_finite() || quotient > MAX_EXACT_TICKS {
        return None;
    }
    let nearest = quotient.round();
    let tolerance =
        (quotient.abs() * f64::EPSILON * STEP_ROUNDOFF_MULTIPLIER).max(STEP_ABSOLUTE_TOLERANCE);
    let count = if (nearest - quotient).abs() <= tolerance {
        nearest
    } else {
        quotient.floor()
    };
    if count < MINIMUM_SLIDER_STEPS {
        return None;
    }
    let upper = (min + count * step).min(max);
    NumericSliderSpec::new(min, upper, step).ok()
}

impl ProjectView {
    pub(super) fn build_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let components = self.snapshot.definition().components.clone();
        for component in &components {
            self.build_component(component, window, cx);
        }
    }

    fn build_component(
        &mut self,
        component: &Component,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &component.kind {
            ComponentKind::Slider { input } => self.build_slider(component, input, window, cx),
            ComponentKind::Select { input } => self.build_select(component, input, window, cx),
            _ => {}
        }
        for child in component.children() {
            self.build_component(child, window, cx);
        }
    }

    fn build_slider(
        &mut self,
        component: &Component,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(InputDeclaration::Number { min, max, step, .. }) =
            self.snapshot.definition().inputs.get(key)
        else {
            return;
        };
        let Some(spec) = slider_spec(*min, *max, *step) else {
            return;
        };
        let Some(InputValue::Number(value)) = self.snapshot.values().get(key) else {
            return;
        };
        let value = *value;
        let state = cx.new(|cx| NumericSliderState::new(spec, value, window, cx));
        state.update(cx, |state, cx| {
            state.set_disabled(!self.snapshot.is_enabled(&component.id), window, cx)
        });
        self.sliders.insert(component.id.clone(), state);
    }

    fn build_select(
        &mut self,
        component: &Component,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = Self::create_select(&self.snapshot, key, window, cx);
        self.retain_select(component, state);
    }

    fn retain_select(&mut self, component: &Component, state: Option<SelectControl>) {
        if let Some(state) = state {
            self.selects.insert(component.id.clone(), state);
        }
    }

    fn create_select(
        snapshot: &LiveState,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<SelectControl> {
        let options = snapshot.options(key)?.to_vec();
        let InputValue::String(value) = snapshot.values().get(key)? else {
            return None;
        };
        let value = value.clone();
        let state = cx.new(|cx| {
            let mut state =
                SelectState::new(SearchableVec::new(options), None, window, cx).searchable(true);
            state.set_selected_value(&value, window, cx);
            state
        });
        let trigger = state.focus_handle(cx);
        Some(SelectControl { state, trigger })
    }

    pub(super) fn subscribe_controls(&mut self, cx: &mut Context<Self>) {
        let components = self.snapshot.definition().components.clone();
        for component in &components {
            self.subscribe_component(component, cx);
        }
    }

    fn subscribe_component(&mut self, component: &Component, cx: &mut Context<Self>) {
        match &component.kind {
            ComponentKind::Slider { input } => self.subscribe_slider(&component.id, input, cx),
            ComponentKind::Select { input } => self.subscribe_select(&component.id, input, cx),
            _ => {}
        }
        for child in component.children() {
            self.subscribe_component(child, cx);
        }
    }

    fn subscribe_slider(&mut self, id: &str, key: &str, cx: &mut Context<Self>) {
        let Some(state) = self.sliders.get(id) else {
            return;
        };
        let id = id.to_owned();
        let key = key.to_owned();
        let generation = self.generation;
        let subscription = cx.subscribe(state, move |view, _, event, cx| {
            let NumericSliderEvent::Commit(value) = event;
            if view.generation == generation {
                view.emit_input(&id, &key, InputValue::Number(*value), cx);
            }
        });
        self.subscriptions.push(subscription);
    }

    fn subscribe_select(&mut self, id: &str, key: &str, cx: &mut Context<Self>) {
        let Some(state) = self.selects.get(id) else {
            return;
        };
        let id = id.to_owned();
        let key = key.to_owned();
        let generation = self.generation;
        let subscription = cx.subscribe(&state.state, move |view, _, event, cx| {
            let SelectEvent::Confirm(value) = event;
            if view.generation == generation
                && let Some(value) = value
            {
                view.emit_input(&id, &key, InputValue::String(value.clone()), cx);
            }
        });
        self.subscriptions.push(subscription);
    }

    pub(super) fn sync_controls(
        &mut self,
        next: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let components = next.definition().components.clone();
        for component in &components {
            self.sync_component(component, next, window, cx);
        }
    }

    fn sync_component(
        &mut self,
        component: &Component,
        next: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &component.kind {
            ComponentKind::Slider { input } => self.sync_slider(component, input, next, window, cx),
            ComponentKind::Select { input } => self.sync_select(component, input, next, window, cx),
            _ => {}
        }
        for child in component.children() {
            self.sync_component(child, next, window, cx);
        }
    }

    fn sync_slider(
        &self,
        component: &Component,
        input: &str,
        next: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(state), Some(InputValue::Number(value))) =
            (self.sliders.get(&component.id), next.values().get(input))
        {
            state.update(cx, |state, cx| {
                state.set_disabled(!next.is_enabled(&component.id), window, cx);
                state.set_value(*value, window, cx);
            });
        }
    }

    fn sync_select(
        &mut self,
        component: &Component,
        input: &str,
        next: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.changed_select_snapshot(component, input, next);
        let state = next.and_then(|next| Self::create_select(next, input, window, cx));
        self.retain_select(component, state);
    }

    fn changed_select_snapshot<'a>(
        &self,
        component: &Component,
        input: &str,
        next: &'a LiveState,
    ) -> Option<&'a LiveState> {
        let options_changed = self.snapshot.options(input) != next.options(input);
        let value_changed = self.snapshot.values().get(input) != next.values().get(input);
        let enabled_changed =
            self.snapshot.is_enabled(&component.id) != next.is_enabled(&component.id);
        let session_changed = self.snapshot.session_id() != next.session_id();
        let revision_changed = self.snapshot.input_revision() != next.input_revision();
        // Select confirms later against its mutable list. A changed binding
        // needs a new emitter, not a new listener on the old entity.
        (options_changed || value_changed || enabled_changed || session_changed || revision_changed)
            .then_some(next)
    }

    pub(super) fn restore_control_focus(
        &self,
        previous: HashMap<String, SelectControl>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let select = previous.iter().find_map(|(id, control)| {
            control
                .state
                .focus_handle(cx)
                .is_focused(window)
                .then_some(id)
        });
        let focus = match select {
            Some(id) => self
                .selects
                .get(id)
                .filter(|_| self.snapshot.is_enabled(id))
                .map(|control| control.state.focus_handle(cx)),
            None => window.focused(cx),
        };
        let retired: Vec<_> = previous
            .iter()
            .filter(|(id, control)| self.selects.get(*id) != Some(*control))
            .flat_map(|(_, control)| [control.trigger.clone(), control.state.focus_handle(cx)])
            .collect();
        Self::focus_control(&focus, window, cx);
        // The retired SDK entity may still be retained by the rendered frame.
        // Repair only focus still owned by it, after its queued confirm/cancel.
        cx.defer_in(window, move |_, window, cx| {
            if retired.iter().any(|handle| handle.is_focused(window)) {
                Self::focus_control(&focus, window, cx);
            }
        });
    }

    fn focus_control(focus: &Option<FocusHandle>, window: &mut Window, cx: &mut App) {
        if let Some(focus) = focus {
            focus.focus(window, cx);
        } else {
            window.blur(cx);
        }
    }
}
