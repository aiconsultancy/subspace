//! The standard searchable initiative menu in the Issues toolbar and palette.

use gpui_kit::{AnyElement, Context, Entity, Window};
use subspace_framework::dashboard::menu::MenuEvent;

use crate::Issues;

const ANCHOR: &str = "issues-initiative";
/// The choice that clears the initiative, in the menu and the navigator.
pub(crate) const CLEAR_LABEL: &str = "All initiatives";

pub(crate) fn button(issues: &Issues, cx: &mut Context<Issues>) -> Option<AnyElement> {
    let catalog = issues.initiatives.read(cx);
    if !catalog.has_memberships() {
        return None;
    }
    let button = catalog
        .button("issues-initiative-button", CLEAR_LABEL)
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    Some(issues.menu_anchors.trigger(ANCHOR, button, cx))
}

pub(crate) fn command(
    issues: &Issues,
    cx: &gpui_kit::App,
) -> Option<subspace_framework::dashboard::PaletteCommand> {
    if !issues.is_ready()
        || issues.editor.is_some()
        || issues.issue_view.snapshot.is_some()
        || !issues.initiatives.read(cx).has_memberships()
    {
        return None;
    }
    let owner = issues.weak.clone();
    Some(
        subspace_framework::dashboard::PaletteCommand::callback(
            "Switch initiative…",
            move |window, cx| {
                _ = owner.update(cx, |board, cx| open(board, window, cx));
            },
        )
        .keywords("issues project team"),
    )
}

fn open(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let catalog = issues.initiatives.read(cx);
    let revision = catalog.revision();
    let contents = catalog.menu(CLEAR_LABEL);
    let anchor = issues.menu_anchors.anchor(ANCHOR, cx);
    let Some(menu) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    subscribe_menu(&menu, revision, cx);
    issues.menu = Some(menu);
    cx.notify();
}

fn subscribe_menu(
    menu: &Entity<subspace_framework::dashboard::menu::Menu>,
    revision: u64,
    cx: &mut Context<Issues>,
) {
    cx.subscribe(menu, move |this, _, event: &MenuEvent, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.menu = None;
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
}
