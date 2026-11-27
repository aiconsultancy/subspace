//! The navigator title follows the app default and the saved choice, and a
//! navigator without it starts with its first section.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, Styled as _,
    TestAppContext, TestSupportExt as _, VisualTestContext, Window, div, px,
};

use super::navigator_title::{install_default, set_shown, shown};
use super::{Navigator, Pane, Workspace, nav_section, strip_height};
use crate::tabs::{TabSpec, pane_tab_bar, tab};
use subspace_lucide::IconName;

/// A workspace whose navigator has a badge and a header action when
/// `extras` is set.
struct Shell {
    extras: bool,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = pane_tab_bar("title-tabs")
            .selected_index(0)
            .child(tab(TabSpec::new("Runs", IconName::ListChecks)));
        let navigator = Navigator::new("Runs").child(nav_section("Views", cx));
        let navigator = if self.extras {
            navigator.badge("Beta").action(
                div()
                    .id("title-action")
                    .test_support()
                    .w(px(20.))
                    .h(px(20.)),
            )
        } else {
            navigator
        };
        div().w(px(1000.)).h(px(600.)).child(
            Workspace::new("title-workspace")
                .navigator(navigator)
                .pane(Pane::new("title-pane").tabs(tabs).child(div().size_full())),
        )
    }
}

fn mount(cx: &mut TestAppContext) -> &mut VisualTestContext {
    mount_with(false, cx)
}

fn mount_with(extras: bool, cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::prefs::init_file(None, cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Shell { extras });
    cx
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn the_title_shows_by_default_and_a_hidden_default_drops_the_header(cx: &mut TestAppContext) {
    let cx = mount(cx);
    draw(cx);
    cx.update(|window, cx| {
        assert!(shown(cx));
        assert!(window.try_find("workspace-navigator-title").is_some());
    });
    cx.update(|_, cx| install_default(false, cx));
    draw(cx);
    cx.update(|window, cx| {
        assert!(!shown(cx));
        assert!(window.try_find("workspace-navigator-header").is_none());
        let navigator = window.find("workspace-navigator").bounds();
        let body = window.find("workspace-navigator-body").bounds();
        assert_eq!(
            body.top(),
            navigator.top(),
            "the first section starts the navigator"
        );
    });
}

#[gpui_kit::test]
fn a_saved_choice_wins_over_the_app_default(cx: &mut TestAppContext) {
    let cx = mount(cx);
    cx.update(|_, cx| {
        install_default(false, cx);
        set_shown(true, cx);
    });
    draw(cx);
    cx.update(|window, cx| {
        assert!(shown(cx));
        assert!(window.try_find("workspace-navigator-title").is_some());
    });
}

#[gpui_kit::test]
fn a_hidden_title_keeps_a_strip_height_header_for_a_badge_or_action(cx: &mut TestAppContext) {
    let cx = mount_with(true, cx);
    cx.update(|_, cx| install_default(false, cx));
    draw(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("workspace-navigator-title").is_none());
        let header = window.find("workspace-navigator-header").bounds();
        assert_eq!(header.size.height, strip_height(window, cx));
        let tabs = window.find("workspace-tab-strip").bounds();
        assert_eq!(header.bottom(), tabs.bottom(), "the strips line up");
        let action = window.find("title-action").bounds();
        assert!(
            header.right() - action.right() < px(16.),
            "the action stays on the trailing edge"
        );
        let tree = window.find("workspace-navigator-body");
        assert_eq!(tree.role(), Some(Role::Tree));
        assert_eq!(
            tree.label(),
            Some("Runs"),
            "the tree keeps the Space's name"
        );
    });
}
