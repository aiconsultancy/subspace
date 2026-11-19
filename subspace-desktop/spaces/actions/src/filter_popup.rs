//! The Actions filter editor. Each field commits immediately to the list.

use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, div,
};
use subspace_framework::dashboard::action::{Intent, icon_button, menu_button};
use subspace_framework::dashboard::{MODAL_CONTEXT, Tokens, anchored};
use subspace_framework::lucide::IconName;

use crate::{
    Actions,
    menus::{self, Picker},
};

#[cfg(test)]
mod tests;

type Choices = SelectState<SearchableVec<String>>;

struct Field {
    kind: Picker,
    choices: Entity<Choices>,
    _selection: Subscription,
}

impl Field {
    fn new(
        actions: &Actions,
        kind: Picker,
        window: &mut Window,
        cx: &mut Context<Actions>,
    ) -> Self {
        let mut values = menus::picker_values(actions, kind);
        let selected = kind
            .filter_value(&actions.filters)
            .unwrap_or_else(|| "All".into());
        if !values.contains(&selected) {
            values.push(selected.clone());
        }
        let choices = cx.new(|cx| {
            let mut state =
                SelectState::new(SearchableVec::new(values), None, window, cx).searchable(true);
            state.set_selected_value(&selected, window, cx);
            state
        });
        let selection = cx.subscribe(&choices, move |actions, _, event, cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                menus::pick(actions, kind, value.clone(), cx);
            }
        });
        Self {
            kind,
            choices,
            _selection: selection,
        }
    }

    fn render(&self, tokens: &Tokens) -> impl IntoElement {
        let label = self.kind.title();
        h_flex()
            .gap_3()
            .child(
                div()
                    .w_20()
                    .flex_shrink_0()
                    .text_color(tokens.muted)
                    .child(label),
            )
            .child(
                div().flex_1().min_w_0().child(
                    anchored::select(&self.choices)
                        .id(self.kind.id())
                        .accessibility_label(label)
                        .small(),
                ),
            )
    }
}

pub(crate) struct FilterPopup {
    owner: WeakEntity<Actions>,
    initial: Option<Picker>,
    fields: [Field; 5],
}

impl FilterPopup {
    fn initial_focus(&self, cx: &App) -> FocusHandle {
        self.fields
            .iter()
            .find(|field| Some(field.kind) == self.initial)
            .unwrap_or(&self.fields[0])
            .choices
            .focus_handle(cx)
    }
}

impl Render for FilterPopup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let owner = self.owner.clone();
        let close = icon_button(
            "actions-filters-close",
            Intent::Quiet,
            IconName::X,
            "Close filters",
        )
        .on_click(move |_, _, cx| _ = owner.update(cx, Actions::close_filters));
        v_flex()
            .id("actions-filters-panel")
            .test_support()
            .key_context(MODAL_CONTEXT)
            .w_80()
            .gap_2()
            .text_size(tokens.row_font)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(tokens.heading_font)
                            .child("Filters"),
                    )
                    .child(close),
            )
            .children(self.fields.iter().map(|field| field.render(&tokens)))
    }
}

pub(crate) fn open(
    actions: &mut Actions,
    initial: Option<Picker>,
    window: &mut Window,
    cx: &mut Context<Actions>,
) {
    if actions.pending.is_some() {
        return;
    }
    actions.menu_anchors.dismiss(window, cx);
    actions.menu = None;
    let fields = Picker::FILTERS.map(|kind| Field::new(actions, kind, window, cx));
    let owner = cx.weak_entity();
    actions.filter_popup = Some(cx.new(|_| FilterPopup {
        owner,
        initial,
        fields,
    }));
    cx.notify();
}

pub(crate) fn button(actions: &Actions, cx: &Context<Actions>) -> impl IntoElement {
    let panel = actions.filter_popup.clone();
    let focus = panel.as_ref().map(|panel| panel.read(cx).initial_focus(cx));
    let owner = cx.weak_entity();
    let mut popup = anchored::form_popover("actions-filters-popover")
        .trigger(menu_button(
            "actions-filters-button",
            IconName::Funnel,
            "Filters",
            actions.filters.narrows() || actions.filters.by_status,
        ))
        .open(panel.is_some())
        .on_open_change(move |is_open, window, cx| {
            _ = owner.update(cx, |actions, cx| {
                if *is_open {
                    open(actions, None, window, cx);
                } else {
                    actions.close_filters(cx);
                }
            });
        })
        .content(move |_, _, _| div().children(panel.clone()));
    if let Some(focus) = focus {
        popup = popup.track_focus(&focus);
    }
    popup
}
