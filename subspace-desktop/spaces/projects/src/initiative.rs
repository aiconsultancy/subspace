//! The same initiative choice that scopes Issues and Channels.

use gpui_kit::component::Disableable as _;
use gpui_kit::{AnyElement, Context, Window};
use subspace_api_client::initiatives::{Catalog, not_a_member};
use subspace_api_client::may_serve_initiative_channels;
use subspace_framework::dashboard::menu::MenuEvent;

use crate::Projects;

const ANCHOR: &str = "projects-initiative";

pub(super) fn scope(catalog: &Catalog) -> (Option<String>, bool) {
    let selected = catalog.selected().map(|initiative| initiative.id.clone());
    let hidden = not_a_member(subspace_api_client::capabilities(), catalog)
        || (selected.is_none()
            && (catalog.reading()
                || (may_serve_initiative_channels() && !catalog.memberships_known())));
    (selected, hidden)
}

pub(super) fn button(this: &Projects, cx: &mut Context<Projects>) -> Option<AnyElement> {
    let catalog = this.initiatives.read(cx);
    if !catalog.has_memberships() {
        return None;
    }
    let button = catalog
        .button("projects-initiative-button")
        .disabled(this.editor.is_some() || this.write_task.is_some())
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    Some(this.menu_anchors.trigger(ANCHOR, button, cx))
}

fn open(this: &mut Projects, window: &mut Window, cx: &mut Context<Projects>) {
    let Some((menu, revision)) =
        Catalog::open_picker(&this.initiatives, &this.menu_anchors, ANCHOR, window, cx)
    else {
        return;
    };
    cx.subscribe(&menu, move |this, _, event: &MenuEvent, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.initiative_menu = None;
        if let MenuEvent::Pick(value) = event {
            this.initiatives.update(cx, |catalog, cx| {
                if catalog.revision() == revision {
                    catalog.pick(value, cx);
                }
            });
        }
        cx.notify();
    })
    .detach();
    this.initiative_menu = Some(menu);
    cx.notify();
}
