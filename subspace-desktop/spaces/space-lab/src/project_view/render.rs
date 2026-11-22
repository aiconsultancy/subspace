use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, anchored, slider};

use super::ProjectView;
use crate::project::{Component, ComponentKind, InputValue, ResultValue};

mod presentation;
use presentation::result_metadata;

impl ProjectView {
    pub(super) fn render_project(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .id("project-content")
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                v_flex().p_3().gap_3().children(
                    self.snapshot
                        .definition()
                        .components
                        .iter()
                        .map(|component| self.render_component(component, &tokens, cx))
                        .collect::<Vec<_>>(),
                ),
            )
    }

    pub(super) fn element_id(&self, component: &Component) -> SharedString {
        format!("project-{}-{}", self.snapshot.definition().id, component.id).into()
    }

    fn render_component(
        &self,
        component: &Component,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match &component.kind {
            ComponentKind::Row { children } => h_flex()
                .items_stretch()
                .min_w_0()
                .gap_3()
                .children(
                    children
                        .iter()
                        .map(|child| self.render_component(child, tokens, cx))
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
            ComponentKind::Column { children } => v_flex()
                .min_w_0()
                .gap_3()
                .children(
                    children
                        .iter()
                        .map(|child| self.render_component(child, tokens, cx))
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
            ComponentKind::Grid { columns, children } => {
                self.render_grid(children, *columns, tokens, cx)
            }
            ComponentKind::Text { text } => {
                let text = match self
                    .snapshot
                    .results()
                    .get(&component.id)
                    .map(|result| &result.value)
                {
                    Some(ResultValue::Text { text }) => text,
                    _ => text,
                };
                div()
                    .id(self.element_id(component))
                    .test_support()
                    .role(gpui_kit::Role::Paragraph)
                    .aria_label(text.clone())
                    .min_w_0()
                    .child(text.clone())
                    .into_any_element()
            }
            ComponentKind::Slider { input } => self.render_slider(component, input, tokens),
            ComponentKind::Select { .. } => self.render_select(component, tokens),
            ComponentKind::Button { action } => self.render_action(component, action, cx),
            ComponentKind::Chart => self.render_chart(component),
            ComponentKind::Table => self.render_table(component),
        };
        result_metadata(
            content,
            self.snapshot.results().get(&component.id),
            &self.element_id(component),
            tokens,
        )
    }

    fn render_grid(
        &self,
        children: &[Component],
        columns: u8,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = children
            .chunks(usize::from(columns))
            .map(|row| {
                h_flex().items_stretch().gap_3().children(
                    row.iter()
                        .map(|child| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.render_component(child, tokens, cx))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        v_flex().min_w_0().gap_3().children(rows).into_any_element()
    }

    fn render_slider(&self, component: &Component, input: &str, tokens: &Tokens) -> AnyElement {
        if let Some(state) = self.sliders.get(&component.id) {
            return slider::control(
                self.element_id(component),
                component.label.clone(),
                state,
                tokens,
            )
            .into_any_element();
        }
        let value = match self.snapshot.values().get(input) {
            Some(InputValue::Number(value)) => value.to_string(),
            _ => "Unavailable".into(),
        };
        v_flex()
            .gap_1()
            .child(component.label.clone())
            .child(value)
            .child(
                div()
                    .text_color(tokens.muted)
                    .child("Readonly: this range cannot be represented by the slider"),
            )
            .into_any_element()
    }

    fn render_select(&self, component: &Component, tokens: &Tokens) -> AnyElement {
        let Some(state) = self.selects.get(&component.id) else {
            return div().child("Select unavailable").into_any_element();
        };
        let enabled = self.snapshot.is_enabled(&component.id);
        v_flex()
            .gap_1()
            .min_w_0()
            .child(
                div()
                    .text_color(tokens.muted)
                    .child(component.label.clone()),
            )
            .child(
                div().w_64().max_w_full().min_w_0().child(
                    anchored::select(&state.state)
                        .id(self.element_id(component))
                        .small()
                        .accessibility_label(component.label.clone())
                        .disabled(!enabled),
                ),
            )
            .into_any_element()
    }

    fn render_action(
        &self,
        component: &Component,
        action: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = component.id.clone();
        let action = action.to_owned();
        let generation = self.generation;
        h_flex()
            .min_w_0()
            .child(
                action_button(
                    self.element_id(component),
                    Intent::Neutral,
                    Some(IconName::Play),
                    component.label.clone(),
                )
                .disabled(!self.snapshot.is_enabled(&component.id))
                .on_click(cx.listener(move |view, _, _, cx| {
                    if view.generation == generation {
                        view.emit_action(&id, &action, cx);
                    }
                })),
            )
            .into_any_element()
    }

    fn render_table(&self, component: &Component) -> AnyElement {
        let Some(grid) = self.grids.get(&component.id) else {
            return v_flex()
                .child(component.label.clone())
                .child("No data")
                .into_any_element();
        };
        v_flex()
            .min_w_0()
            .gap_1()
            .child(grid.view.clone())
            .child(grid.feedback.clone())
            .into_any_element()
    }
}
