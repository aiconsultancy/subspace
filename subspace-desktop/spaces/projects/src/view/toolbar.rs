//! The pane's toolbar is its strip. Back leads when a milestone is open. The initiative and project views
//! move into the overflow menu as the pane narrows.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{App, Context, ParentElement as _, Styled as _, WeakEntity, div};
use subspace_framework::dashboard::{
    action::{Intent, action_button, icon_button},
    overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry},
    workspace,
};
use subspace_framework::lucide::IconName;

use crate::{Projects, Tab, initiative};

pub(super) fn render(this: &mut Projects, cx: &mut Context<Projects>) -> workspace::Toolbar {
    let units = leading_units(this, cx);
    let leading = OverflowToolbar::sync(&mut this.leading, units, cx);
    // A hidden initiative picker opens from the overflow menu's chevron.
    this.menu_anchors.set_toolbar(&leading);
    let mut bar = workspace::Toolbar::new("projects-toolbar").strip();
    if let Some(detail) = &this.milestone_pages.detail {
        let label = match detail.from {
            Tab::Overview => "Back to overview",
            Tab::Milestones => "Back to milestones",
        };
        bar = bar.leading(
            icon_button("milestone-back", Intent::Quiet, IconName::ArrowLeft, label)
                .on_click(cx.listener(|this, _, _, cx| this.close_milestone(cx))),
        );
    }
    bar.leading(div().min_w_0().max_w_full().child(leading))
}

/// The initiative and the open project's two views, in that order.
fn leading_units(this: &Projects, cx: &mut Context<Projects>) -> Vec<OverflowUnit> {
    let owner = cx.entity().downgrade();
    let initiative = initiative_unit(this, owner.clone(), cx);
    let overview = page_unit(this, Tab::Overview, owner.clone());
    let milestones = page_unit(this, Tab::Milestones, owner);
    [initiative, overview, milestones]
        .into_iter()
        .flatten()
        .collect()
}

/// The initiative picker, once there are initiatives to pick from. Its key
/// is the picker's anchor, so a hidden picker opens from the chevron.
fn initiative_unit(this: &Projects, owner: WeakEntity<Projects>, cx: &App) -> Option<OverflowUnit> {
    let catalog = this.initiatives.read(cx);
    if !catalog.has_memberships() {
        return None;
    }
    let opener = owner.clone();
    Some(catalog.overflow_unit(
        initiative::ANCHOR,
        this.editing_held(),
        move |_, cx| {
            owner
                .update(cx, |this, cx| initiative::button(this, cx))
                .ok()
                .flatten()
        },
        move |window, cx| {
            _ = opener.update(cx, |this, cx| initiative::open(this, window, cx));
        },
    ))
}

/// Overview and Milestones, with the current view selected.
fn page_unit(this: &Projects, tab: Tab, owner: WeakEntity<Projects>) -> Option<OverflowUnit> {
    this.selected.as_ref()?;
    let blocked = this.editor.is_some();
    let label = match tab {
        Tab::Overview => "Overview",
        Tab::Milestones => "Milestones",
    };
    let open = tab == this.tab;
    let show = move |owner: &WeakEntity<Projects>, cx: &mut App| {
        _ = owner.update(cx, |this, cx| show_page(this, tab, cx));
    };
    let menu_owner = owner.clone();
    let entry = PopupEntry::new(move || {
        let owner = menu_owner.clone();
        PopupMenuItem::new(label)
            .checked(open)
            .disabled(blocked)
            .on_click(move |_, _, cx| show(&owner, cx))
    });
    Some(OverflowUnit::inline(
        label,
        OverflowUnit::UNMEASURED_WIDTH,
        move |_, _| {
            let owner = owner.clone();
            action_button(label, Intent::Quiet, None, label)
                .latched(open)
                .disabled(blocked)
                .on_click(move |_, _, cx| show(&owner, cx))
        },
        vec![entry],
    ))
}

/// Shows the open project's overview or its milestones, leaving a
/// milestone's page.
fn show_page(this: &mut Projects, tab: Tab, cx: &mut Context<Projects>) {
    this.milestone_pages.detail = None;
    this.milestone_pages.preview = None;
    this.tab = tab;
    cx.notify();
}
