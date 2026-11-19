//! Shared confirmation frame. Target facts are frozen when the intent opens.

use std::collections::BTreeMap;
use subspace_framework::client_github::actions::DispatchInput;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::InputState;
use gpui_kit::{AnyElement, AppContext as _, Context, Div, Entity, ParentElement as _, Window};
use subspace_framework::dashboard::action::{Intent, dialog_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::fields::{self, FieldLayout, LabelledInput};
use subspace_framework::dashboard::{Tokens, rich};
use subspace_framework::lucide::IconName;

use crate::{Actions, model::Record, writes::Operation};

mod dispatch;
pub(crate) use dispatch::load_definition;
pub(crate) use dispatch::values;
#[cfg(test)]
mod runtime_tests;

pub(crate) struct DraftInput {
    pub spec: DispatchInput,
    pub value: Entity<InputState>,
}

pub(crate) struct Confirmation {
    pub operation: Operation,
    pub record: Record,
    pub reference: Entity<InputState>,
    pub inputs: Entity<InputState>,
    pub comment: Entity<InputState>,
    pub busy: bool,
    pub error: Option<String>,
    pub definition: Definition,
}

/// What a dispatch knows about the workflow's declared inputs.
pub(crate) enum Definition {
    /// Only a dispatch reads the workflow definition.
    NotNeeded,
    Loading,
    Readable {
        fields: Vec<DraftInput>,
        /// The workflow declares a `workflow_dispatch` trigger.
        dispatchable: bool,
    },
    /// Inputs are sent as unvalidated JSON.
    Unreadable,
}

impl Definition {
    pub(crate) fn blocks_submit(&self) -> bool {
        matches!(
            self,
            Self::Loading
                | Self::Readable {
                    dispatchable: false,
                    ..
                }
        )
    }

    pub(crate) fn fields(&self) -> &[DraftInput] {
        match self {
            Self::Readable { fields, .. } => fields,
            _ => &[],
        }
    }
}

impl Confirmation {
    pub(crate) fn new(
        operation: Operation,
        record: Record,
        window: &mut Window,
        cx: &mut Context<Actions>,
    ) -> Self {
        let branch = record
            .run
            .as_ref()
            .map(|run| run.branch.clone())
            .unwrap_or_default();
        Self {
            operation,
            record,
            reference: cx.new(|cx| InputState::new(window, cx).default_value(branch)),
            inputs: cx.new(|cx| InputState::new(window, cx).default_value("{}")),
            comment: cx.new(|cx| InputState::new(window, cx)),
            busy: false,
            error: None,
            definition: if operation == Operation::Dispatch {
                Definition::Loading
            } else {
                Definition::NotNeeded
            },
        }
    }
}

pub(crate) fn parse_inputs(text: &str) -> Result<BTreeMap<String, String>, String> {
    serde_json::from_str(text)
        .map_err(|_| "Inputs must be a flat JSON object of string values".into())
}

pub(crate) fn render(this: &Actions, cx: &mut Context<Actions>) -> Option<AnyElement> {
    let pending = this.pending.as_ref()?;
    let tokens = Tokens::current(cx);
    let body = dialog::frame(
        &tokens,
        DialogWidth::D540,
        format!("Confirm {}", pending.operation.title()),
    )
    .child(rich::plain_text(
        "actions-confirm-target",
        &format!("{} · {}", pending.record.repo, pending.record.title),
    ))
    .children(form(pending, &tokens, cx))
    .children(
        pending
            .error
            .as_ref()
            .map(|error| rich::plain_text("actions-write-error", error)),
    )
    .child(footer(
        pending.busy || pending.definition.blocks_submit(),
        cx,
    ));
    Some(dialog::backdrop(&tokens, body))
}

fn form(pending: &Confirmation, tokens: &Tokens, cx: &mut Context<Actions>) -> Option<Div> {
    match pending.operation {
        Operation::Dispatch => Some(dispatch::form(pending, tokens, cx)),
        Operation::Approve | Operation::Reject => Some(fields::labelled_input(
            LabelledInput::new(&pending.comment, "Review comment", FieldLayout::Stacked),
            tokens,
        )),
        _ => None,
    }
}

fn footer(busy: bool, cx: &mut Context<Actions>) -> Div {
    dialog::footer()
        .child(
            dialog_button(
                "actions-confirm-cancel",
                Intent::Neutral,
                Some(IconName::X),
                "Back",
            )
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| {
                this.pending = None;
                cx.notify();
            })),
        )
        .child(
            dialog_button(
                "actions-confirm-submit",
                Intent::Primary,
                Some(IconName::Check),
                if busy { "Applying…" } else { "Confirm" },
            )
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| this.confirm_write(cx))),
        )
}

#[cfg(test)]
mod tests {
    use super::parse_inputs;

    #[test]
    fn fallback_inputs_only_accept_flat_string_objects() {
        assert_eq!(
            parse_inputs(r#"{"environment":"qat"}"#).unwrap()["environment"],
            "qat"
        );
        for invalid in ["[]", "null", r#"{"count":1}"#, r#"{"nested":{}}"#] {
            assert!(parse_inputs(invalid).is_err());
        }
    }
}
