//! GPUI Kit layout regions: description lists, group boxes, resizable
//! panes, scrollable areas, sidebars, status bars, toolbars, the title bar
//! and focus traps.

use gpui_kit::component::description_list::{DescriptionItem, DescriptionList};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::toolbar::{Toolbar, ToolbarGroup};
use gpui_kit::component::{FocusTrapElement as _, Sizable as _, TitleBar, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, ignore_click, number, sample, sized, store_number};

const WIDE: f32 = 420.;
const HALF: f32 = 210.;
const PANES: (f32, f32) = (420., 120.);
const SCROLL: (f32, f32) = (260., 120.);
const SIDEBAR: (f32, f32) = (260., 220.);
const BAR: (f32, f32) = (420., 40.);
const PANE_SIZE: f32 = 140.;
const PANE_MIN: f32 = 80.;
const PANE_MAX: f32 = 300.;
const LINES: usize = 24;
const SIDEBAR_KEY: &str = "kit-sidebar";
const DESTINATIONS: [(&str, &str); 3] = [
    ("gallery-kit:sidebar-inbox", "Inbox"),
    ("gallery-kit:sidebar-drafts", "Drafts"),
    ("gallery-kit:sidebar-archive", "Archive"),
];

pub(crate) fn description_lists(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let items = [
        ("Name", "Sample fixture"),
        ("Owner", "Ada Example"),
        ("Board", "component-gallery"),
        ("Updated", "Today"),
    ];
    vec![sample(
        "Bordered, two columns",
        sized(
            WIDE,
            DescriptionList::new()
                .bordered(true)
                .columns(2)
                .children(items.map(|(label, value)| DescriptionItem::new(label).value(value))),
        ),
    )]
}

pub(crate) fn group_boxes(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let body = || div().child("Related settings grouped together.");
    vec![
        sample(
            "Titled",
            sized(HALF, GroupBox::new().title("Updates").child(body())),
        ),
        sample(
            "Outline",
            sized(
                HALF,
                GroupBox::new().outline().title("Updates").child(body()),
            ),
        ),
    ]
}

fn pane(label: &'static str, tokens: &Tokens) -> impl IntoElement {
    div()
        .size_full()
        .p_2()
        .text_color(tokens.muted)
        .child(label)
}

pub(crate) fn resizables(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let group = h_resizable("gallery-kit:resizable")
        .with_state(&gallery.retained.kit.layout.panes)
        .child(
            resizable_panel()
                .size(px(PANE_SIZE))
                .size_range(px(PANE_MIN)..px(PANE_MAX))
                .child(pane("Drag the edge", tokens)),
        )
        .child(resizable_panel().child(pane("The rest", tokens)));
    vec![sample("Two panes", bounded(PANES, tokens, group))]
}

pub(crate) fn scrollables(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let lines = (1..=LINES).map(|line| div().px_2().child(format!("Fixture line {line}")));
    vec![sample(
        "Vertical scrollbar",
        bounded(
            SCROLL,
            tokens,
            v_flex()
                .id("gallery-kit:scrollable")
                .size_full()
                .overflow_y_scrollbar()
                .children(lines),
        ),
    )]
}

pub(crate) fn sidebars(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = gallery.retained.values.clone();
    let active = number(&values, SIDEBAR_KEY, 0, cx);
    let items = DESTINATIONS.iter().enumerate().map(|(at, (_, label))| {
        let values = values.clone();
        SidebarMenuItem::new(*label)
            .icon(IconName::Folder)
            .active(at == active)
            .on_click(move |_, _, cx| store_number(&values, SIDEBAR_KEY, at, cx))
    });
    let sidebar = Sidebar::new("gallery-kit:sidebar")
        .collapsible(false)
        .w_full()
        .child(SidebarGroup::new("Mail").child(SidebarMenu::new().children(items)));
    vec![sample(
        "Click a destination",
        bounded(SIDEBAR, tokens, sidebar),
    )]
}

pub(crate) fn status_bars(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let bar = StatusBar::new()
        .left(
            action::action_button(
                "gallery-kit:status-branch",
                Intent::Quiet,
                Some(IconName::GitBranch),
                "main",
            )
            .compact()
            .flat()
            .on_click(ignore_click),
        )
        .right(div().text_xs().child("Ln 12, Col 4"));
    vec![sample(
        "Leading and trailing regions",
        bounded(BAR, tokens, bar),
    )]
}

pub(crate) fn toolbars(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let icon = |id: &'static str, icon| {
        action::icon_button(id, Intent::Quiet, icon, id).on_click(ignore_click)
    };
    let toolbar = Toolbar::new("gallery-kit:toolbar")
        .small()
        .child(
            ToolbarGroup::new("gallery-kit:toolbar-history")
                .label("History")
                .child(icon("Undo", IconName::Undo2))
                .child(icon("Redo", IconName::Redo2)),
        )
        .child(
            ToolbarGroup::new("gallery-kit:toolbar-format")
                .label("Format")
                .child(icon("Bold", IconName::Bold))
                .child(icon("Italic", IconName::Italic)),
        );
    vec![sample("Grouped commands", sized(WIDE, toolbar))]
}

/// GPUI Kit's TitleBar moves the window when dragged and, on Windows and
/// Linux, draws working window controls, so the sample sits under a cover
/// that takes the pointer.
pub(crate) fn title_bars(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let bar = div()
        .relative()
        .size_full()
        .child(TitleBar::new().child(div().child("Fixture window")))
        .child(
            div()
                .id("gallery-kit:title-bar-cover")
                .absolute()
                .inset_0()
                .occlude(),
        );
    vec![sample(
        "Shown only: dragging would move the window",
        bounded(BAR, tokens, bar),
    )]
}

pub(crate) fn focus_traps(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.layout;
    let trap = v_flex()
        .id("gallery-kit:focus-trap")
        .focus_trap("gallery-kit:focus-trap", &state.trap)
        .gap_2()
        .p_2()
        .border_1()
        .border_color(tokens.line)
        .rounded(tokens.control_radius)
        .child(Input::new(&state.trap_first))
        .child(Input::new(&state.trap_second));
    vec![sample(
        "Click a field, then Tab: focus stays inside",
        sized(HALF, trap),
    )]
}
