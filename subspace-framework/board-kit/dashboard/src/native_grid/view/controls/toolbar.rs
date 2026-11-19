use super::{NativeGrid, scope_label};
use crate::{
    action::{Intent, action_button},
    icon_button::Metrics,
    native_grid::{GridFormat, GridScope},
    overflow_toolbar::{OverflowUnit, PopupEntry},
};
use gpui_kit::component::{Disableable as _, Selectable as _, menu::PopupMenuItem};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div,
};
use subspace_lucide::IconName;
#[cfg(test)]
mod tests;

// Positive logical-pixel estimate, replaced by OverflowToolbar's natural-width measurement.
const INITIAL_UNIT_WIDTH_PX: f32 = 1.;

#[derive(Clone, Copy)]
enum Action {
    Copy(GridScope),
    ClearSort,
    ClearFilters,
    Options,
}

struct Command {
    id: SharedString,
    label: SharedString,
    icon: IconName,
    disabled: bool,
    selected: bool,
    action: Action,
}

pub(super) fn render(grid: &NativeGrid, cx: &mut Context<NativeGrid>) -> impl IntoElement {
    let owner = cx.weak_entity();
    let mut units: Vec<_> = [GridScope::Selected, GridScope::View, GridScope::All]
        .into_iter()
        .map(|scope| command_unit(command(grid, Action::Copy(scope)), owner.clone()))
        .collect();
    units.push(export_unit(grid, owner.clone()));
    for action in [Action::ClearSort, Action::ClearFilters, Action::Options] {
        units.push(command_unit(command(grid, action), owner.clone()));
    }
    grid.toolbar
        .update(cx, |toolbar, cx| toolbar.set_units(units, cx));
    div()
        .id(grid.child_id("toolbar"))
        .test_support()
        .w_full()
        .min_w_0()
        .min_h(Metrics::current(cx).target())
        .child(grid.toolbar.clone())
}

fn command(grid: &NativeGrid, action: Action) -> Command {
    let (suffix, label, icon, disabled, selected) = match action {
        Action::Copy(scope) => (
            format!("copy-{}", scope_label(scope)),
            format!("Copy {}", scope_label(scope)),
            IconName::Copy,
            !grid.options[7]
                || (scope == GridScope::Selected && grid.model.selection.cells.is_empty()),
            false,
        ),
        Action::ClearSort => (
            "clear-sort".into(),
            "Clear sort".into(),
            IconName::X,
            grid.model.sort.is_none(),
            false,
        ),
        Action::ClearFilters => (
            "clear-filters".into(),
            "Clear filters".into(),
            IconName::X,
            grid.model.filters.iter().all(String::is_empty),
            false,
        ),
        Action::Options => (
            "options".into(),
            "Grid options".into(),
            IconName::Settings2,
            false,
            grid.options_open,
        ),
    };
    Command {
        id: grid.child_id(suffix),
        label: label.into(),
        icon,
        disabled,
        selected,
        action,
    }
}

fn command_unit(command: Command, owner: WeakEntity<NativeGrid>) -> OverflowUnit {
    let popup_owner = owner.clone();
    let action = command.action;
    let label = command.label.clone();
    let icon = command.icon;
    let disabled = command.disabled;
    let selected = command.selected;
    OverflowUnit::inline(
        command.id.clone(),
        INITIAL_UNIT_WIDTH_PX,
        move |_, _| {
            let owner = owner.clone();
            action_button(
                command.id.clone(),
                Intent::Quiet,
                Some(command.icon),
                command.label.clone(),
            )
            .disabled(disabled)
            .selected(selected)
            .on_click(move |_, window, cx| {
                _ = owner.update(cx, |grid, cx| grid.activate_toolbar(action, window, cx));
            })
        },
        vec![PopupEntry::new(move || {
            let owner = popup_owner.clone();
            PopupMenuItem::new(label.clone())
                .icon(icon)
                .disabled(disabled)
                .checked(selected)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |grid, cx| grid.activate_toolbar(action, window, cx));
                })
        })],
    )
}

fn export_unit(grid: &NativeGrid, owner: WeakEntity<NativeGrid>) -> OverflowUnit {
    let selected = !grid.model.selection.cells.is_empty();
    let mut entries = Vec::new();
    for scope in [GridScope::Selected, GridScope::View, GridScope::All] {
        for format in [GridFormat::Csv, GridFormat::Json] {
            let owner = owner.clone();
            let label = format!(
                "Export {} as {}…",
                scope_label(scope),
                if format == GridFormat::Csv {
                    "CSV"
                } else {
                    "JSON"
                }
            );
            entries.push(PopupEntry::new(move || {
                let owner = owner.clone();
                PopupMenuItem::new(label.clone())
                    .icon(IconName::Download)
                    .disabled(scope == GridScope::Selected && !selected)
                    .on_click(move |_, _, cx| {
                        _ = owner.update(cx, |grid, cx| grid.export(scope, format, cx));
                    })
            }));
        }
    }
    OverflowUnit::inline(
        grid.child_id("export"),
        INITIAL_UNIT_WIDTH_PX,
        move |_, cx| {
            owner
                .update(cx, |grid, cx| grid.export_menu(cx).into_any_element())
                .unwrap_or_else(|_| div().into_any_element())
        },
        entries,
    )
}

impl NativeGrid {
    fn activate_toolbar(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Copy(scope) => self.copy(scope, cx),
            Action::ClearSort => self.clear_sort(cx),
            Action::ClearFilters => self.clear_filters(window, cx),
            Action::Options => {
                self.options_open = !self.options_open;
                cx.notify();
            }
        }
    }
}
