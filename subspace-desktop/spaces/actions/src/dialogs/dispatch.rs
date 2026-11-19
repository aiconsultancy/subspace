//! Typed workflow inputs share normal fields and exclusive-choice controls.
use subspace_framework::client_github::actions::ActionsApi as _;

use std::collections::BTreeMap;

use gpui_kit::component::input::InputState;
use gpui_kit::{
    App, AppContext as _, Context, Div, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Window, div,
};
use subspace_framework::client_github::actions::{Definition, DispatchInputType};
use subspace_framework::dashboard::fields::{self, FieldLayout, LabelledInput};
use subspace_framework::dashboard::{Tokens, rich, segmented};
use subspace_framework::lucide::IconName;

use super::{Confirmation, Definition as Inputs, DraftInput};
use crate::Actions;

pub(crate) fn load_definition(this: &mut Actions, window: &mut Window, cx: &mut Context<Actions>) {
    let Some(target) = this.pending.as_ref().and_then(|p| p.record.run.clone()) else {
        return;
    };
    let (Some(client), Some(login)) = (this.client.clone(), this.login.clone()) else {
        return;
    };
    let generation = this.generation;
    cx.spawn_in(window, async move |this, cx| {
        let fetch_target = target.clone();
        let result = cx
            .background_spawn(async move {
                let path = fetch_target
                    .path
                    .split('@')
                    .next()
                    .unwrap_or(&fetch_target.path);
                client.actions_workflow_definition(
                    &login,
                    &fetch_target.repo,
                    &fetch_target.sha,
                    path,
                )
            })
            .await;
        _ = this.update_in(cx, |this, window, cx| {
            if generation != this.generation {
                return;
            }
            let Some(pending) = &mut this.pending else {
                return;
            };
            if pending.record.run.as_ref() != Some(&target) {
                return;
            }
            apply_definition(pending, result, window, cx);
            cx.notify();
        });
    })
    .detach();
}

fn apply_definition(
    pending: &mut Confirmation,
    result: Result<
        Definition<subspace_framework::client_github::actions::WorkflowDefinition>,
        subspace_framework::core::http::Error,
    >,
    window: &mut Window,
    cx: &mut Context<Actions>,
) {
    match result {
        Ok(Definition::Readable(definition)) => {
            let dispatchable = definition.dispatch.is_some();
            let fields = definition
                .dispatch
                .into_iter()
                .flat_map(|dispatch| dispatch.inputs)
                .map(|spec| {
                    let default = spec.default.clone().unwrap_or_else(|| {
                        if spec.kind == DispatchInputType::Boolean {
                            "false".into()
                        } else {
                            String::new()
                        }
                    });
                    let value = cx.new(|cx| InputState::new(window, cx).default_value(default));
                    DraftInput { spec, value }
                })
                .collect();
            pending.definition = Inputs::Readable {
                fields,
                dispatchable,
            };
            if !dispatchable {
                pending.error = Some("This workflow has no workflow_dispatch trigger".into());
            }
        }
        Ok(Definition::Unreadable(reason)) => {
            pending.definition = Inputs::Unreadable;
            pending.error = Some(format!(
                "Definition unreadable: {reason}. Use unvalidated JSON inputs."
            ))
        }
        Err(error) => {
            pending.definition = Inputs::Unreadable;
            pending.error = Some(format!(
                "Definition unavailable: {error}. Use unvalidated JSON inputs."
            ))
        }
    }
}

pub(super) fn form(pending: &Confirmation, tokens: &Tokens, cx: &mut Context<Actions>) -> Div {
    let base = div().child(fields::labelled_input(
        LabelledInput::new(&pending.reference, "Branch or tag", FieldLayout::Stacked),
        tokens,
    ));
    let drafts = match &pending.definition {
        Inputs::Loading => {
            return base.child(rich::plain_text(
                "actions-definition-loading",
                "Reading workflow inputs…",
            ));
        }
        Inputs::Readable { fields, .. } => fields,
        Inputs::NotNeeded | Inputs::Unreadable => {
            return base
                .child(fields::labelled_input(
                    LabelledInput::new(
                        &pending.inputs,
                        "Workflow inputs JSON",
                        FieldLayout::Stacked,
                    ),
                    tokens,
                ))
                .child(rich::plain_text(
                    "actions-input-notice",
                    "Inputs are not validated against the workflow definition.",
                ));
        }
    };
    base.children(
        drafts
            .iter()
            .enumerate()
            .map(|(index, field)| field_control(index, field, tokens, cx)),
    )
}

fn field_control(
    index: usize,
    field: &DraftInput,
    tokens: &Tokens,
    cx: &mut Context<Actions>,
) -> Div {
    let options = match field.spec.kind {
        DispatchInputType::Boolean => vec!["false".to_owned(), "true".to_owned()],
        DispatchInputType::Choice => field.spec.options.clone(),
        _ => {
            return fields::labelled_input(
                LabelledInput::new(&field.value, field.spec.name.clone(), FieldLayout::Stacked),
                tokens,
            );
        }
    };
    let selected = options
        .iter()
        .position(|option| field.value.read(cx).value() == option.as_str());
    let items = options
        .iter()
        .map(|option| segmented::Segment::new(option.clone()).icon(IconName::Check))
        .collect::<Vec<_>>();
    let weak = cx.entity().downgrade();
    div().child(
        div()
            .id(("actions-input", index))
            .child(field.spec.name.clone())
            .child(
                segmented::control(
                    "choices",
                    items,
                    selected,
                    tokens,
                    move |selected, _, window, cx| {
                        _ = weak.update(cx, |this, cx| {
                            if let Some(field) = this
                                .pending
                                .as_ref()
                                .and_then(|pending| pending.definition.fields().get(index))
                                && let Some(value) = options.get(selected)
                            {
                                field.value.update(cx, |input, cx| {
                                    input.set_value(value.clone(), window, cx)
                                });
                            }
                            cx.notify();
                        });
                    },
                )
                .into_any_element(),
            ),
    )
}

pub(crate) fn values(pending: &Confirmation, cx: &App) -> Result<BTreeMap<String, String>, String> {
    let Inputs::Readable { fields, .. } = &pending.definition else {
        return super::parse_inputs(&pending.inputs.read(cx).value());
    };
    let mut values = BTreeMap::new();
    for field in fields {
        let value = field.value.read(cx).value().to_string();
        if field.spec.required && value.trim().is_empty() {
            return Err(format!("{} is required", field.spec.name));
        }
        if field.spec.kind == DispatchInputType::Choice && !field.spec.options.contains(&value) {
            return Err(format!("Select a declared choice for {}", field.spec.name));
        }
        if field.spec.kind == DispatchInputType::Boolean
            && !["true", "false"].contains(&value.as_str())
        {
            return Err(format!("Choose true or false for {}", field.spec.name));
        }
        values.insert(field.spec.name.clone(), value);
    }
    Ok(values)
}
