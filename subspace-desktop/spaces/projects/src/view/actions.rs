//! The trailing side of the pane's toolbar: Refresh, then the page's own
//! action at the right edge, where every Space keeps its primary action. In
//! a narrow pane the strip's leading side keeps room for its overflow button,
//! so these yield the rest of the line and move into their own overflow
//! menu, the page action first.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{
    AnyElement, App, Context, IntoElement as _, ParentElement as _, Styled as _, WeakEntity,
    Window, div,
};
use subspace_framework::dashboard::{
    Dashboard as _,
    action::{Intent, action_button, main_action},
    overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry},
};
use subspace_framework::lucide::IconName;

use crate::{Projects, Tab};

const NEW_MILESTONE: &str = "New milestone";
const EDIT_PROJECT: &str = "Edit project";

/// Refresh and the page's action, as one overflowing row.
pub(super) fn render(this: &mut Projects, cx: &mut Context<Projects>) -> AnyElement {
    let owner = cx.entity().downgrade();
    let page = page_action(this, owner.clone());
    let units = std::iter::once(refresh_unit(owner)).chain(page).collect();
    let trailing = OverflowToolbar::sync(&mut this.trailing, units, cx);
    div()
        .min_w_0()
        .max_w_full()
        .child(trailing)
        .into_any_element()
}

/// What a page's action does when chosen.
#[derive(Clone, Copy)]
enum PageAction {
    NewMilestone,
    EditProject,
}

impl PageAction {
    /// New milestone on the milestones page, or Edit project on the
    /// overview; nothing on a milestone's page.
    fn of(this: &Projects) -> Option<Self> {
        if this.milestone_pages.detail.is_some() {
            None
        } else if this.tab == Tab::Milestones && this.may_write() {
            Some(Self::NewMilestone)
        } else if this.tab == Tab::Overview && this.editable_project().is_some() {
            Some(Self::EditProject)
        } else {
            None
        }
    }

    fn run(self, owner: &WeakEntity<Projects>, window: &mut Window, cx: &mut App) {
        _ = owner.update(cx, |this, cx| match self {
            Self::NewMilestone => this.open_editor(None, window, cx),
            Self::EditProject => this.open_project_editor(window, cx),
        });
    }
}

/// The page's action, the Space's main one on the milestones page.
fn page_action(this: &Projects, owner: WeakEntity<Projects>) -> Option<OverflowUnit> {
    let action = PageAction::of(this)?;
    let blocked = this.editor.is_some();
    let (id, icon, label) = match action {
        PageAction::NewMilestone => ("projects-new-milestone", IconName::Plus, NEW_MILESTONE),
        PageAction::EditProject => ("projects-edit", IconName::Pencil, EDIT_PROJECT),
    };
    let menu_owner = owner.clone();
    let entry = PopupEntry::new(move || {
        let owner = menu_owner.clone();
        PopupMenuItem::new(label)
            .icon(icon)
            .disabled(blocked)
            .on_click(move |_, window, cx| action.run(&owner, window, cx))
    });
    Some(OverflowUnit::inline(
        id,
        OverflowUnit::UNMEASURED_WIDTH,
        move |_, _| {
            let owner = owner.clone();
            let button = match action {
                PageAction::NewMilestone => main_action(id, icon, label),
                PageAction::EditProject => action_button(id, Intent::Neutral, Some(icon), label),
            };
            button
                .disabled(blocked)
                .on_click(move |_, window, cx| action.run(&owner, window, cx))
        },
        vec![entry],
    ))
}

/// Refresh, before the page's action: it reads the projects again.
fn refresh_unit(owner: WeakEntity<Projects>) -> OverflowUnit {
    OverflowUnit::icon_button(
        "projects-refresh",
        IconName::RefreshCw,
        "Refresh",
        move |window, cx| {
            _ = owner.update(cx, |this, cx| this.reload(window, cx));
        },
    )
}
