//! Overflow projections preserve every toolbar action at narrow widths.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use subspace_framework::dashboard::action::{Intent, action_button, menu_button};
use subspace_framework::dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_framework::dashboard::tabs::{self, TabSpec};
use subspace_framework::dashboard::{Dashboard as _, list};
use subspace_framework::lucide::IconName;

use crate::{Actions, filter_popup, menus::Picker, model::View, writes::Operation};

const UNMEASURED_WIDTH: f32 = 0.;
/// Use the same content edge as the Issues list.
pub(crate) const LIST_SIDE_INSET: Pixels = px(24.);

pub(crate) fn render(this: &mut Actions, cx: &mut Context<Actions>) -> impl IntoElement + use<> {
    let controls = controls(this, cx);
    v_flex()
        .id("actions-list-toolbar")
        .test_support()
        .w_full()
        .flex_shrink_0()
        .child(list::header(
            Actions::ID,
            Actions::TITLE,
            scope_line(this),
            LIST_SIDE_INSET,
            cx,
        ))
        .child(
            v_flex()
                .px(LIST_SIDE_INSET)
                .pb_2()
                .gap_2()
                .child(view_tabs(this.view, cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(filter_popup::button(this, cx))
                        .child(div().min_w_0().child(controls)),
                ),
        )
}

fn scope_line(actions: &Actions) -> String {
    match actions.login.as_deref() {
        Some(login) => format!("GitHub · Signed in as {login}"),
        None if actions.fetch.loads_records() => "GitHub · Checking account…".into(),
        None if actions.error.is_some() => "GitHub · Account unavailable".into(),
        None => "GitHub".into(),
    }
}

fn controls(this: &mut Actions, cx: &mut Context<Actions>) -> Entity<OverflowToolbar> {
    let weak = cx.entity().downgrade();
    let mut units = [Picker::Account, Picker::Log]
        .into_iter()
        .map(|kind| picker_unit(weak.clone(), kind))
        .collect::<Vec<_>>();
    units.extend(Operation::ALL.into_iter().map(|operation| {
        write_unit(
            weak.clone(),
            operation,
            crate::writes::unavailable(operation, this.selected()),
        )
    }));
    let toolbar = OverflowToolbar::sync(&mut this.toolbar, units, cx);
    this.menu_anchors.set_toolbar(&toolbar);
    toolbar
}

fn view_tabs(selected: View, cx: &Context<Actions>) -> tabs::TabBar {
    tabs::tab_bar("actions-tabs")
        .w_full()
        .selected_index(selected as usize)
        .children(View::ALL.map(|view| tabs::tab(TabSpec::new(view.title(), view.icon()))))
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            if let Some(view) = View::ALL.get(*index) {
                this.set_view(*view, cx);
            }
        }))
}

fn picker_unit(weak: WeakEntity<Actions>, kind: Picker) -> OverflowUnit {
    let popup_weak = weak.clone();
    let popup = PopupEntry::anchored(move |anchor| {
        let weak = popup_weak.clone();
        PopupMenuItem::new(kind.title())
            .icon(IconName::Funnel)
            .on_click(move |_, window, cx| {
                _ = weak.update(cx, |this, cx| {
                    crate::menus::open_picker(this, kind, Some(anchor.host()), window, cx)
                });
            })
    });
    OverflowUnit::inline(
        kind.id(),
        UNMEASURED_WIDTH,
        move |_, cx| {
            let weak = weak.clone();
            let label = picker_label(&weak, kind, cx);
            let button = menu_button(
                format!("actions-picker-{}", kind.title()),
                IconName::Funnel,
                label,
                false,
            )
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    _ = weak.update(cx, |this, cx| {
                        crate::menus::open_picker(this, kind, None, window, cx)
                    });
                }
            });
            weak.update(cx, |this, cx| {
                this.menu_anchors.trigger(kind.id(), button, cx)
            })
            .unwrap_or_else(|_| gpui_kit::div().into_any_element())
        },
        vec![popup],
    )
}

fn picker_label(weak: &WeakEntity<Actions>, kind: Picker, cx: &App) -> String {
    if kind == Picker::Account {
        weak.read_with(cx, |this, _| this.login.clone())
            .ok()
            .flatten()
            .unwrap_or_else(|| kind.title().into())
    } else {
        kind.title().into()
    }
}

fn write_unit(
    weak: WeakEntity<Actions>,
    operation: Operation,
    reason: Option<&'static str>,
) -> OverflowUnit {
    let popup_weak = weak.clone();
    let popup = PopupEntry::new(move || {
        let weak = popup_weak.clone();
        let label = reason.map_or_else(
            || operation.title().to_owned(),
            |reason| format!("{}: {reason}", operation.title()),
        );
        PopupMenuItem::new(label)
            .icon(operation.icon())
            .disabled(reason.is_some())
            .on_click(move |_, window, cx| {
                _ = weak.update(cx, |this, cx| this.begin_write(operation, window, cx));
            })
    });
    OverflowUnit::inline(
        format!("actions-write-{}", operation.title()),
        UNMEASURED_WIDTH,
        move |_, cx| {
            let weak = weak.clone();
            let reason = weak
                .read_with(cx, |this, _| {
                    crate::writes::unavailable(operation, this.selected())
                })
                .ok()
                .flatten();
            action_button(
                format!("actions-intent-{}", operation.title()),
                Intent::Neutral,
                Some(operation.icon()),
                operation.title(),
            )
            .disabled(reason.is_some())
            .tooltip(reason.unwrap_or(operation.title()))
            .on_click(move |_, window, cx| {
                _ = weak.update(cx, |this, cx| this.begin_write(operation, window, cx));
            })
            .into_any_element()
        },
        vec![popup],
    )
}
