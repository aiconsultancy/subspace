use crate::Issues;
use gpui_kit::{AnyElement, Context, Window};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::IconName;

pub(super) fn render(issues: &Issues, cx: &mut Context<Issues>) -> AnyElement {
    // Lit only while its own menu is open, not the end-state or any other
    // menu on the page.
    let lit = issues
        .menu_anchors
        .anchor("issue-more", cx)
        .read(cx)
        .is_open();
    let button = menu_button("issue-more", IconName::Ellipsis, "More", lit)
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    issues.menu_anchors.trigger("issue-more", button, cx)
}

fn open(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let contents = MenuContents::new(
        "More",
        "Find an action",
        vec![
            MenuOption::new("copy", "Copy identifier"),
            MenuOption::new("reload", "Reload"),
        ],
    );
    let anchor = issues.menu_anchors.anchor("issue-more", cx);
    let Some(menu) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&menu, |this, _, event: &MenuEvent, cx| {
        match event {
            MenuEvent::Pick(value) => {
                if value.as_ref() == "copy" {
                    this.copy_issue_identifier(cx);
                }
                if value.as_ref() == "reload" {
                    this.reload_detail(cx);
                }
                this.menu = None;
            }
            MenuEvent::Close => this.menu = None,
            MenuEvent::Query(_) => {}
        }
        cx.notify();
    })
    .detach();
    issues.menu = Some(menu);
    cx.notify();
}
