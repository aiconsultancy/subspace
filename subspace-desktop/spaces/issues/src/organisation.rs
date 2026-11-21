//! The NETT organisation menu in the navigator's header, or in the toolbar
//! while the navigator is hidden. It shows only in a NETT build whose
//! account has several business organisations; choosing one retires the
//! hub's data for the previous organisation and starts again.

use gpui_kit::{AnyElement, Context, Pixels, Styled as _, Window, px};
use subspace_api_client::nett::{self, Snapshot};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::IconName;

use crate::Issues;

const ANCHOR: &str = "issues-organisation";
const NONE_LABEL: &str = "Choose organisation";
const MAX_WIDTH: Pixels = px(240.);

/// The organisations to offer, when there is a choice to make.
fn choices() -> Option<Snapshot> {
    if subspace_api_client::backend() != subspace_api_client::Backend::Nett {
        return None;
    }
    let snapshot = nett::snapshot();
    (snapshot.organisations.len() > 1).then_some(snapshot)
}

/// The selected organisation's name, or the prompt to choose one.
pub(crate) fn label(snapshot: &Snapshot) -> String {
    snapshot
        .selected
        .as_ref()
        .and_then(|id| snapshot.organisations.iter().find(|org| &org.id == id))
        .map_or_else(|| NONE_LABEL.to_owned(), |org| org.name.clone())
}

pub(crate) fn menu(snapshot: &Snapshot) -> MenuContents {
    let options = snapshot
        .organisations
        .iter()
        .map(|org| {
            MenuOption::new(org.id.clone(), org.name.clone())
                .checked(snapshot.selected.as_ref() == Some(&org.id))
        })
        .collect();
    MenuContents::new("Organisation", "Filter organisations", options)
}

pub(crate) fn button(issues: &Issues, cx: &mut Context<Issues>) -> Option<AnyElement> {
    let snapshot = choices()?;
    let label = label(&snapshot);
    let button = menu_button(
        "issues-organisation-button",
        IconName::Building2,
        label.clone(),
        false,
    )
    .max_w(MAX_WIDTH)
    .accessibility_label(format!("Organisation: {label}"))
    .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    Some(issues.menu_anchors.trigger(ANCHOR, button, cx))
}

fn open(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(snapshot) = choices() else {
        return;
    };
    let anchor = issues.menu_anchors.anchor(ANCHOR, cx);
    let Some(menu) = issues
        .menu_anchors
        .open(anchor, menu(&snapshot), window, cx)
    else {
        return;
    };
    let generation = snapshot.generation;
    cx.subscribe(&menu, move |this, _, event: &MenuEvent, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.menu = None;
        if let MenuEvent::Pick(id) = event
            && nett::snapshot().generation == generation
        {
            nett::select_organisation(id);
        }
        cx.notify();
    })
    .detach();
    issues.menu = Some(menu);
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::{NONE_LABEL, label, menu};
    use subspace_api_client::nett::{Organisation, Snapshot, Status};

    fn snapshot(selected: Option<&str>) -> Snapshot {
        let orgs: Vec<Organisation> = serde_json::from_value(serde_json::json!([
            {"id": "00000000-0000-0000-0000-000000000001", "name": "Fixture Works"},
            {"id": "00000000-0000-0000-0000-000000000002", "name": "Second Fixture Org"},
        ]))
        .unwrap();
        Snapshot::new(orgs, selected.map(str::to_owned), Status::Choose, 1)
    }

    #[test]
    fn the_menu_names_the_selected_organisation_or_asks_for_one() {
        assert_eq!(label(&snapshot(None)), NONE_LABEL);
        let chosen = snapshot(Some("00000000-0000-0000-0000-000000000002"));
        assert_eq!(label(&chosen), "Second Fixture Org");
        assert_eq!(menu(&chosen).label, "Organisation");
    }
}
