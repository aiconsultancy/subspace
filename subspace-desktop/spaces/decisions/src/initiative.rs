//! Decisions follows the same account-scoped initiative selection as the
//! other tracker Spaces and Channels.

use gpui_kit::{AnyElement, Context, Entity, IntoElement as _, Subscription};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::model::Project;
use subspace_framework::dashboard::workspace::{self, NavRow};
use subspace_framework::lucide::IconName;

use crate::Decisions;

pub(crate) fn connect(
    cx: &mut Context<Decisions>,
) -> (Entity<Catalog>, Option<String>, Subscription) {
    let catalog = Catalog::shared(cx);
    let selected = selected_id(catalog.read(cx));
    let subscription = cx.observe(&catalog, |board, catalog, cx| {
        let selected = selected_id(catalog.read(cx));
        if selected != board.initiative_id {
            board.initiative_id = selected;
            board.rescope_initiative(cx);
        }
        cx.notify();
    });
    (catalog, selected, subscription)
}

fn selected_id(catalog: &Catalog) -> Option<String> {
    catalog.selected().map(|initiative| initiative.id.clone())
}

pub(crate) fn includes(project: &Project, initiative: Option<&str>) -> bool {
    initiative.is_none_or(|id| project.initiative_id.as_deref() == Some(id))
}

pub(crate) fn rows(board: &Decisions, cx: &Context<Decisions>) -> Vec<AnyElement> {
    let options = board.initiatives.read(cx).menu().options;
    if options.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![workspace::nav_section("Initiatives", cx).into_any_element()];
    rows.extend(options.into_iter().map(|option| {
        let value = option.value;
        NavRow::new(format!("decisions-nav-initiative:{value}"), option.label)
            .icon(IconName::Users)
            .selected(option.checked)
            .on_click(cx.listener(move |board, _, _, cx| {
                board
                    .initiatives
                    .update(cx, |catalog, cx| catalog.pick(&value, cx));
                board.show_list(cx);
            }))
            .into_any_element()
    }));
    rows
}
