//! The pane's toolbar, which is its strip: the pane has no tab strip. The
//! initiative, Overview, Milestones and Back keep to one line; in a narrow
//! pane, or at a large icon-button size, the last of them move into the
//! overflow menu at its end.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, Styled as _, WeakEntity, Window, div,
};
use subspace_framework::dashboard::{
    action::{Intent, action_button},
    overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry},
    workspace,
};
use subspace_framework::lucide::IconName;

use crate::{Projects, Tab, initiative};

/// Units are measured when they first draw.
const UNMEASURED_WIDTH: f32 = 0.;
const BACK: &str = "Back";

pub(super) fn render(this: &mut Projects, cx: &mut Context<Projects>) -> workspace::Toolbar {
    let units = leading_units(this, cx);
    let leading = OverflowToolbar::sync(&mut this.leading, units, cx);
    // A hidden initiative picker opens from the overflow menu's chevron.
    this.menu_anchors.set_toolbar(&leading);
    workspace::Toolbar::new("projects-toolbar")
        .strip()
        .leading(div().min_w_0().max_w_full().child(leading))
}

/// The initiative, the open project's two pages and, on a milestone's page,
/// Back, in that order.
fn leading_units(this: &Projects, cx: &mut Context<Projects>) -> Vec<OverflowUnit> {
    let owner = cx.entity().downgrade();
    let initiative = initiative_unit(this, owner.clone(), cx);
    let overview = page_unit(this, Tab::Overview, owner.clone());
    let milestones = page_unit(this, Tab::Milestones, owner.clone());
    let back = back_unit(this, owner);
    [initiative, overview, milestones, back]
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
    let name = catalog
        .selected()
        .map_or_else(|| "Initiative".to_owned(), |item| item.name.clone());
    let disabled = this.editing_held();
    let opener = owner.clone();
    let entry = PopupEntry::new(move || {
        let opener = opener.clone();
        PopupMenuItem::new(format!("Initiative: {name}…"))
            .icon(IconName::Users)
            .disabled(disabled)
            .on_click(move |_, window, cx| {
                _ = opener.update(cx, |this, cx| initiative::open(this, window, cx));
            })
    });
    Some(OverflowUnit::inline(
        initiative::ANCHOR,
        UNMEASURED_WIDTH,
        move |_, cx| {
            owner
                .update(cx, |this, cx| initiative::button(this, cx))
                .ok()
                .flatten()
                .unwrap_or_else(|| div().into_any_element())
        },
        vec![entry],
    ))
}

/// Overview or Milestones while a project is selected, the open one in the
/// primary style.
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
    let intent = if open { Intent::Primary } else { Intent::Quiet };
    Some(OverflowUnit::inline(
        label,
        UNMEASURED_WIDTH,
        move |_, _| {
            let owner = owner.clone();
            action_button(label, intent, None, label)
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

/// Back from a milestone's page, while one is open, to the page it was
/// opened from.
fn back_unit(this: &Projects, owner: WeakEntity<Projects>) -> Option<OverflowUnit> {
    this.milestone_pages.detail.as_ref()?;
    let back = |owner: &WeakEntity<Projects>, cx: &mut App| {
        _ = owner.update(cx, |this, cx| this.close_milestone(cx));
    };
    let menu_owner = owner.clone();
    let entry = PopupEntry::new(move || {
        let owner = menu_owner.clone();
        PopupMenuItem::new(BACK)
            .icon(IconName::ArrowLeft)
            .on_click(move |_, _, cx| back(&owner, cx))
    });
    Some(OverflowUnit::inline(
        "milestone-back",
        UNMEASURED_WIDTH,
        move |_, _| {
            let owner = owner.clone();
            action_button(
                "milestone-back",
                Intent::Neutral,
                Some(IconName::ArrowLeft),
                BACK,
            )
            .on_click(move |_, _: &mut Window, cx| back(&owner, cx))
        },
        vec![entry],
    ))
}
