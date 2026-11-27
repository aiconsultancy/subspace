//! The strips along the top of the workspace are one height, so the
//! hairlines under them are one line, and no two borders meet on one edge.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Context, IntoElement, ParentElement as _, Pixels, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};
use subspace_lucide::IconName;

use super::{Inspector, InspectorTab, Navigator, Pane, Toolbar, Workspace, strip_height};
use crate::action::{Intent, action_button, icon_button, main_action};
use crate::density::Density;
use crate::icon_button::IconButtonSize;
use crate::panel::{self, PanelActions, PanelHeader};
use crate::tabs::{TabSpec, pane_tab_bar, tab};

/// Two edges closer than this are one line.
const SAME_LINE_PX: f32 = 0.5;

/// Both docks and a pane, headed by a tab strip or by a toolbar standing in
/// for one. With the toolbar as the strip, the toolbar ends in a labelled
/// button and the inspector carries tabs in place of its title.
struct Shell {
    tabs: bool,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let toolbar = Toolbar::new("strip-toolbar").leading(icon_button(
            "strip-search",
            Intent::Quiet,
            IconName::Search,
            "Search",
        ));
        let pane = Pane::new("strip-pane");
        let pane = if self.tabs {
            let tabs = pane_tab_bar("strip-tabs")
                .selected_index(0)
                .child(tab(TabSpec::new("Backlog", IconName::ListChecks)))
                .child(tab(TabSpec::new("SUB-12", IconName::CircleDot)));
            pane.tabs(tabs).toolbar(toolbar)
        } else {
            let new = main_action("strip-new", IconName::Plus, "New note…");
            let archive = action_button("strip-archive", Intent::Neutral, None, "Archive");
            pane.toolbar(toolbar.trailing(archive).trailing(new).strip())
        };
        let navigator = Navigator::new("Notes").action(icon_button(
            "strip-add",
            Intent::Quiet,
            IconName::Plus,
            "Add",
        ));
        let inspector = Inspector::new("strip-inspector", "Note").on_close("Close", |_, _, _| {});
        let inspector = if self.tabs {
            inspector
        } else {
            let tabs = [
                InspectorTab::new("Note"),
                InspectorTab::new("People").count("5"),
            ];
            inspector.tabs(tabs, 0, |_, _, _, _| {})
        };
        div().w(px(1200.)).h(px(600.)).child(
            Workspace::new("strip-workspace")
                .navigator(navigator)
                .pane(pane.child(div().size_full()))
                .inspector(inspector),
        )
    }
}

fn mount(tabs: bool, cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(move |_, _| Shell { tabs });
    cx
}

fn bounds(window: &mut Window, cx: &mut gpui_kit::App, id: &'static str) -> Bounds<Pixels> {
    window.render_frame(cx);
    window.find(id).bounds()
}

fn assert_one_line(edges: &[(&str, Pixels)]) {
    let (first, line) = edges[0];
    for (name, edge) in edges {
        assert!(
            (f32::from(*edge) - f32::from(line)).abs() < SAME_LINE_PX,
            "{name} ends at {edge:?}, {first} at {line:?}"
        );
    }
}

/// The bottoms of the docks' headers and of the pane's top strip.
fn strip_bottoms(
    pane_strip: &'static str,
    cx: &mut VisualTestContext,
) -> [(&'static str, Pixels); 3] {
    cx.update(|window, cx| {
        [
            (
                "workspace-navigator-header",
                bounds(window, cx, "workspace-navigator-header").bottom(),
            ),
            (pane_strip, bounds(window, cx, pane_strip).bottom()),
            (
                "workspace-inspector-header",
                bounds(window, cx, "workspace-inspector-header").bottom(),
            ),
        ]
    })
}

#[gpui_kit::test]
fn the_dock_headers_and_the_tab_strip_end_on_one_line(cx: &mut TestAppContext) {
    let cx = mount(true, cx);
    let bottoms = strip_bottoms("workspace-tab-strip", cx);
    assert_one_line(&bottoms);
    cx.update(|window, cx| {
        let strip = bounds(window, cx, "workspace-tab-strip");
        assert_eq!(
            strip.size.height,
            strip_height(window, cx),
            "exactly the strip height"
        );
    });
}

#[gpui_kit::test]
fn a_toolbar_standing_in_for_the_tab_strip_ends_on_the_same_line(cx: &mut TestAppContext) {
    let cx = mount(false, cx);
    assert_one_line(&strip_bottoms("strip-toolbar", cx));
}

#[gpui_kit::test]
fn larger_icon_buttons_grow_every_strip_together(cx: &mut TestAppContext) {
    let cx = mount(true, cx);
    cx.update(|_, cx| IconButtonSize::Large.apply(cx));
    let bottoms = strip_bottoms("workspace-tab-strip", cx);
    assert_one_line(&bottoms);
    cx.update(|window, cx| {
        let token = crate::Tokens::current(cx).strip_height;
        let header = bounds(window, cx, "workspace-navigator-header");
        assert!(
            header.size.height > token,
            "the strips grew to hold the buttons"
        );
        let button = bounds(window, cx, "workspace-inspector-close");
        assert!(
            button.bottom() <= header.bottom(),
            "the close button fits its strip"
        );
    });
}

#[gpui_kit::test]
fn the_first_pane_tab_draws_no_line_of_its_own_at_the_dock_edge(cx: &mut TestAppContext) {
    let cx = mount(true, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let pane = window.find("workspace-pane").bounds();
        let navigator = window.find("workspace-navigator").bounds();
        assert!(
            (f32::from(navigator.right()) - f32::from(pane.left())).abs() < SAME_LINE_PX,
            "the pane starts at the navigator's edge, which draws the line"
        );
        let mut bar = window.within("strip-tabs");
        let first = bar.within(0usize).find("tab-layers").bounds();
        assert!(
            (f32::from(first.left()) - f32::from(pane.left())).abs() < SAME_LINE_PX,
            "the open tab's fill meets the navigator's line: {first:?} in {pane:?}"
        );
        let second_tab = bar.find(1usize).bounds();
        let second = bar.within(1usize).find("tab-layers").bounds();
        assert!(
            second.left() > second_tab.left(),
            "between tabs, the line is the tabs' own"
        );
    });
}

#[gpui_kit::test]
fn labelled_buttons_and_inspector_tabs_fit_their_strips_at_a_large_size(cx: &mut TestAppContext) {
    let cx = mount(false, cx);
    cx.update(|_, cx| IconButtonSize::Large.apply(cx));
    assert_one_line(&strip_bottoms("strip-toolbar", cx));
    cx.update(|window, cx| {
        let toolbar = bounds(window, cx, "strip-toolbar");
        let button = bounds(window, cx, "strip-new");
        assert!(inside(button, toolbar), "{button:?} in {toolbar:?}");
        let header = bounds(window, cx, "workspace-inspector-header");
        let tabs = bounds(window, cx, "workspace-inspector-tabs");
        assert!(inside(tabs, header), "{tabs:?} in {header:?}");
    });
}

/// Whether `part` lies within `strip` from top to bottom.
fn inside(part: Bounds<Pixels>, strip: Bounds<Pixels>) -> bool {
    part.top() >= strip.top() && part.bottom() <= strip.bottom()
}

#[gpui_kit::test]
fn action_buttons_taller_than_toolbar_buttons_grow_every_strip_together(cx: &mut TestAppContext) {
    let cx = mount(false, cx);
    cx.update(|window, cx| {
        cx.set_global(Density::from_json(
            r#"{"--action-button-h":"40px","--toolbar-button-h":"24px"}"#,
        ));
        window.refresh();
    });
    assert_one_line(&strip_bottoms("strip-toolbar", cx));
    cx.update(|window, cx| {
        let toolbar = bounds(window, cx, "strip-toolbar");
        let archive = bounds(window, cx, "strip-archive");
        let search = bounds(window, cx, "strip-search");
        assert!(
            archive.size.height > search.size.height,
            "the action button {archive:?} is the taller, over {search:?}"
        );
        assert!(inside(archive, toolbar), "{archive:?} in {toolbar:?}");
        assert!(
            archive.top() - toolbar.top() >= px(super::FOCUS_RING_PX),
            "room for the focus ring above"
        );
    });
}

/// A pane with a tab strip beside the shared detail panel.
struct DetailShell;

impl Render for DetailShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = pane_tab_bar("detail-tabs")
            .selected_index(0)
            .child(tab(TabSpec::new("Runs", IconName::ListChecks)));
        let detail = panel::frame(
            PanelHeader {
                kind: "Run".into(),
                id: "run-1".into(),
                ..Default::default()
            },
            div().child("Body"),
            PanelActions::new(|_, _| {}, None),
            window,
            cx,
        );
        div().w(px(1200.)).h(px(600.)).child(
            Workspace::new("detail-workspace")
                .navigator(Navigator::new("Runs"))
                .pane(Pane::new("detail-pane").tabs(tabs).child(div().size_full()))
                .detail(detail),
        )
    }
}

#[gpui_kit::test]
fn the_detail_panel_bar_ends_level_with_the_tab_strip_at_a_large_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        IconButtonSize::Large.apply(cx);
    });
    let (_, cx) = cx.add_window_view(|_, _| DetailShell);
    for _ in 0..4 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        let edges = [
            "workspace-navigator-header",
            "workspace-tab-strip",
            "panel-toolbar",
        ]
        .map(|id| (id, bounds(window, cx, id).bottom()));
        assert_one_line(&edges);
        let bar = bounds(window, cx, "panel-toolbar");
        let close = bounds(window, cx, "panel-close");
        assert!(inside(close, bar), "{close:?} in {bar:?}");
        assert_eq!(bar.size.height, strip_height(window, cx));
    });
}
