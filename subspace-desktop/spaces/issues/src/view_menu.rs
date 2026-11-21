//! The toolbar's view menu, which stands in for the navigator's Views while
//! the navigator is hidden. It uses the shared filterable menu, as the
//! project picker does.

use gpui_kit::{AnyElement, Context, Window};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};

use crate::Issues;
use crate::navigator::View;

const ANCHOR: &str = "issues-view";

/// The toolbar's view control, named after the view that shows.
pub(crate) fn button(issues: &Issues, cx: &mut Context<Issues>) -> AnyElement {
    let view = View::current(issues.tab, issues.activity.visible);
    let button = menu_button("issues-view-button", view.icon(), view.label(), false)
        .accessibility_label(format!("View: {}", view.label()))
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    issues.menu_anchors.trigger(ANCHOR, button, cx)
}

/// The views as menu options, with `current` checked.
pub(crate) fn options(current: View) -> Vec<MenuOption> {
    View::ALL
        .into_iter()
        .map(|view| MenuOption::new(view.key(), view.label()).checked(view == current))
        .collect()
}

fn open(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let current = View::current(issues.tab, issues.activity.visible);
    let contents = MenuContents::new("View", "Find a view", options(current));
    let anchor = issues.menu_anchors.anchor(ANCHOR, cx);
    let Some(menu) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe_in(&menu, window, |this, _, event: &MenuEvent, window, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.menu = None;
        if let MenuEvent::Pick(value) = event
            && let Some(view) = View::from_key(value)
        {
            this.choose_view(view, window, cx);
        }
        cx.notify();
    })
    .detach();
    issues.menu = Some(menu);
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::options;
    use crate::model::Tab;
    use crate::navigator::View;

    #[test]
    fn the_menu_offers_every_view_and_checks_the_current_one() {
        let options = options(View::Activity);
        let labels: Vec<_> = options.iter().map(|option| option.label.as_ref()).collect();
        assert_eq!(
            labels,
            ["My issues", "Active", "Backlog", "All issues", "Activity"]
        );
        let checked: Vec<_> = options
            .iter()
            .filter(|option| option.checked)
            .map(|option| option.value.as_ref())
            .collect();
        assert_eq!(checked, ["activity"]);
        let backlog = super::options(View::Tab(Tab::Backlog));
        assert!(backlog[2].checked && !backlog[4].checked);
    }
}
