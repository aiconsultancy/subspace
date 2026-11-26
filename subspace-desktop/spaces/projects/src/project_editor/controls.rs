//! The shared fields in the project edit dialog.

use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{Div, Entity, ParentElement as _, Styled as _, div};
use subspace_framework::dashboard::{
    anchored,
    fields::{self, InputRole},
};

use super::Form;

pub(super) fn form_body(form: &Form, busy: bool) -> Div {
    v_flex()
        .gap_3()
        .child(input_field("Name", &form.name, busy))
        .child(fields::form_field(
            "Description",
            fields::textarea(&form.description, "Description").disabled(busy),
        ))
        .child(
            h_flex()
                .gap_3()
                .child(div().min_w_0().flex_1().child(fields::form_field(
                    "Status",
                    anchored::select(&form.status).disabled(busy),
                )))
                .child(div().min_w_0().flex_1().child(fields::form_field(
                    "Priority",
                    anchored::select(&form.priority).disabled(busy),
                ))),
        )
        .child(
            h_flex()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(input_field("Start date", &form.start, busy)),
                )
                .child(div().min_w_0().flex_1().child(input_field(
                    "Target date",
                    &form.target,
                    busy,
                ))),
        )
}

fn input_field(label: &'static str, state: &Entity<InputState>, busy: bool) -> Div {
    fields::form_field(
        label,
        fields::input(state, InputRole::Configuration, label).disabled(busy),
    )
}
