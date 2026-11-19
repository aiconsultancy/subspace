//! The version-pin and package/tool action row beneath the active
//! environment summary.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::h_flex;
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, Window};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, FieldLayout, InputRole, LabelledInput};
use subspace_framework::dashboard::panel::IconName;

use super::super::super::Workbench;
use super::super::dims::{PACKAGE_FIELD_WIDTH, VERSION_FIELD_WIDTH};

pub(super) fn pin_and_package_row(
    this: &mut Workbench,
    _window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let busy = this.jobs.busy;
    let id = this
        .selected_env
        .clone()
        .unwrap_or_else(|| this.draft.environment.clone());
    h_flex()
        .gap_3()
        .pt_3()
        .flex_wrap()
        .border_t_1()
        .border_color(t.hair)
        .child(version_pin_field(this, id.clone(), busy, t, cx))
        .child(package_and_tool_fields(this, id, busy, t, cx))
}

fn version_pin_field(
    this: &Workbench,
    id: String,
    busy: bool,
    t: &Tokens,
    cx: &mut Context<Workbench>,
) -> impl IntoElement {
    h_flex()
        .items_end()
        .gap_2()
        .child(
            fields::labelled_input(
                LabelledInput::new(
                    &this.inputs.version_input,
                    "Pin version",
                    FieldLayout::Stacked,
                )
                .role(InputRole::Configuration),
                t,
            )
            .w(VERSION_FIELD_WIDTH),
        )
        .child(
            action_button(
                "workbench-set-version",
                Intent::Neutral,
                Some(IconName::Pencil),
                "Set version",
            )
            .disabled(busy)
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, _, cx| {
                    let version = this.inputs.version_input.read(cx).value().to_string();
                    this.set_version(id.clone(), version, cx);
                }
            })),
        )
}

fn package_and_tool_fields(
    this: &Workbench,
    id: String,
    busy: bool,
    t: &Tokens,
    cx: &mut Context<Workbench>,
) -> impl IntoElement {
    h_flex()
        .items_end()
        .gap_2()
        .flex_wrap()
        .child(
            fields::labelled_input(
                LabelledInput::new(
                    &this.inputs.package_input,
                    "Add package",
                    FieldLayout::Stacked,
                )
                .role(InputRole::Configuration),
                t,
            )
            .w(PACKAGE_FIELD_WIDTH),
        )
        .child(
            action_button(
                "workbench-add-package",
                Intent::Neutral,
                Some(IconName::Plus),
                "Add package",
            )
            .disabled(busy)
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, _, cx| {
                    let package = this.inputs.package_input.read(cx).value().to_string();
                    this.add_package(id.clone(), package, cx);
                }
            })),
        )
        .child(
            action_button(
                "workbench-add-tool",
                Intent::Neutral,
                Some(IconName::Download),
                "Add mise tool (name@version)",
            )
            .disabled(busy)
            .on_click(cx.listener(move |this, _, _, cx| {
                let request = this.inputs.package_input.read(cx).value().to_string();
                this.add_tool(id.clone(), request, cx)
            })),
        )
}
