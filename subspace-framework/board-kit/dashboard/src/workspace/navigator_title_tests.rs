//! The navigator title follows the app default and the saved choice, and a
//! navigator without it starts with its first section.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::navigator_title::{install_default, set_shown, shown};
use super::{Navigator, Pane, Workspace, nav_section};
use crate::tabs::{TabSpec, pane_tab_bar, tab};
use subspace_lucide::IconName;

struct Shell;

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = pane_tab_bar("title-tabs")
            .selected_index(0)
            .child(tab(TabSpec::new("Runs", IconName::ListChecks)));
        div().w(px(1000.)).h(px(600.)).child(
            Workspace::new("title-workspace")
                .navigator(Navigator::new("Runs").child(nav_section("Views", cx)))
                .pane(Pane::new("title-pane").tabs(tabs).child(div().size_full())),
        )
    }
}

fn mount(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::prefs::init_file(None, cx);
    });
    let (_, cx) = cx.add_window_view(|_, _| Shell);
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
