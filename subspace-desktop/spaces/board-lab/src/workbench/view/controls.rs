//! The controls panel: title field, bound parameter controls, add-control
//! buttons, chart-kind selector and the run/save/delete action row.

use gpui_kit::component::v_flex;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    Context, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, FieldLayout, LabelledInput};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabSpec};
use subspace_framework::dashboard::typography;
use subspace_framework::dashboard::{Tokens, anchored};

use super::super::{ADDABLE_PARAMETERS, Workbench};

pub(super) fn controls_panel(
    this: &mut Workbench,
    _window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let busy = this.jobs.busy;
    let heading_font = typography::heading_font(cx);
    let draft_id = this.draft.id.clone();
    let can_delete = this.presets.iter().any(|preset| preset.id == draft_id);
    let control_fields = bound_control_fields(this, t, busy);
    let missing_controls = missing_control_buttons(this, busy, cx);

    v_flex()
        .gap_2()
        .child(
            div()
                .font_family(heading_font)
                .font_weight(FontWeight::MEDIUM)
                .text_size(t.heading_font)
                .text_color(t.fg_strong)
                .child("Controls"),
        )
        .child(fields::labelled_input(
            LabelledInput::new(&this.inputs.title_input, "Title", FieldLayout::Stacked),
            t,
        ))
        .children(control_fields)
        .child(h_flex().gap_2().flex_wrap().children(missing_controls))
        .child(view_kind_row(this, cx))
        .child(action_row(this, draft_id, can_delete, busy, cx))
}

/// Renders the one widget already bound for each control on the draft, in
/// draft order: a select for `kind == "select"`, otherwise a text input.
fn bound_control_fields<'a>(
    this: &'a Workbench,
    t: &'a Tokens,
    busy: bool,
) -> impl Iterator<Item = gpui_kit::AnyElement> + 'a {
    this.draft.controls.iter().filter_map(move |control| {
        let key = ADDABLE_PARAMETERS
            .iter()
            .find(|(name, _)| *name == control.parameter)
            .map_or("group_by", |(name, _)| *name);
        if let Some(state) = this.inputs.param_selects.get(key) {
            return Some(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(t.row_font)
                            .text_color(t.muted)
                            .child(control.label.clone()),
                    )
                    .child(
                        anchored::select(state)
                            .small()
                            .accessibility_label(control.label.clone())
                            .disabled(busy),
                    )
                    .into_any_element(),
            );
        }
        this.inputs.param_inputs.get(key).map(|state| {
            fields::labelled_input(
                LabelledInput::new(state, control.label.clone(), FieldLayout::Stacked),
                t,
            )
            .into_any_element()
        })
    })
}

fn missing_control_buttons(
    this: &Workbench,
    busy: bool,
    cx: &Context<Workbench>,
) -> Vec<subspace_framework::dashboard::icon_button::IconButton> {
    ADDABLE_PARAMETERS
        .iter()
        .filter(|(name, _)| !this.draft.controls.iter().any(|c| c.parameter == *name))
        .map(move |(name, label)| {
            let parameter = *name;
            action_button(
                SharedString::from(format!("workbench-add-control-{name}")),
                Intent::Neutral,
                Some(IconName::CalendarRange),
                format!("Add {label}"),
            )
            .disabled(busy || this.draft.recipe == "snapshot")
            .on_click(
                cx.listener(move |this, _, window, cx| this.add_control(parameter, window, cx)),
            )
        })
        .collect()
}

fn action_row(
    this: &Workbench,
    draft_id: String,
    can_delete: bool,
    busy: bool,
    cx: &mut Context<Workbench>,
) -> impl IntoElement {
    h_flex()
        .gap_2()
        .flex_wrap()
        .pt_2()
        .child(
            action_button(
                "workbench-run",
                Intent::Neutral,
                Some(IconName::Play),
                "Run",
            )
            .disabled(busy || this.draft.recipe == "snapshot")
            .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
        )
        .child(
            action_button(
                "workbench-save",
                Intent::Primary,
                Some(IconName::Save),
                "Save / Update",
            )
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
        )
        .child(
            action_button(
                "workbench-delete",
                Intent::Reject,
                Some(IconName::Trash),
                "Delete",
            )
            .disabled(busy || !can_delete)
            .on_click(cx.listener(move |this, _, _, cx| this.delete(draft_id.clone(), cx))),
        )
}

fn view_kind_row(this: &mut Workbench, cx: &mut Context<Workbench>) -> impl IntoElement {
    let kinds = ["bar", "line", "area"];
    let tabs = [
        dashboard_tabs::tab(TabSpec::new("Bar", IconName::ChartColumn)),
        dashboard_tabs::tab(TabSpec::new("Line", IconName::ChartArea)),
        dashboard_tabs::tab(TabSpec::new("Area", IconName::ChartArea)),
    ];
    let selected = kinds
        .iter()
        .position(|k| *k == this.draft.view)
        .unwrap_or(0);
    dashboard_tabs::tab_bar("workbench-view-kind")
        .selected_index(selected)
        .children(tabs)
        .on_click(cx.listener(move |this, index: &usize, _, cx| {
            if let Some(kind) = kinds.get(*index) {
                this.draft.view = (*kind).to_owned();
                cx.notify();
            }
        }))
}
