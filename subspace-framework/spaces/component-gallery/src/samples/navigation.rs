//! Navigation and chrome: glass capsules and controls, the overflow
//! toolbar, the surface header and the semantic tokens.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Selectable as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _, Styled as _,
    Window, div, px,
};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_dashboard::surface_header::SurfaceHeader;
use subspace_dashboard::{Tokens, glass};
use subspace_lucide::{IconName, SvgIcon};

use super::{Sample, bounded, ignore_click, sample, sized};
use crate::ComponentGallery;
use crate::catalog::Item;

const GLASS: &str = "board-kit/dashboard/src/glass.rs";
const CAPSULE_HEIGHT: f32 = 28.;
const TOOLBAR_WIDTH: f32 = 260.;
const HEADER_BOX: (f32, f32) = (420., 48.);
const SWATCH: f32 = 18.;
const SWATCH_WIDTH: f32 = 120.;
const SWATCH_ROW: f32 = 520.;
const UNMEASURED: f32 = 0.;
const DESTINATIONS: [(&str, &str, bool); 3] = [
    ("gallery-sample:nav-prs", "PRs", true),
    ("gallery-sample:nav-issues", "Issues", false),
    ("gallery-sample:nav-irc", "Channels", false),
];
const ACTIONS: [(&str, &str, SvgIcon); 5] = [
    (
        "gallery-sample:overflow-refresh",
        "Refresh",
        IconName::RefreshCw,
    ),
    ("gallery-sample:overflow-filter", "Filter", IconName::Funnel),
    (
        "gallery-sample:overflow-sort",
        "Sort",
        IconName::ArrowUpDown,
    ),
    ("gallery-sample:overflow-share", "Share", IconName::Share2),
    (
        "gallery-sample:overflow-export",
        "Export",
        IconName::Download,
    ),
];

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "glass::nav_button",
        source: GLASS,
        used_in: "The shell's board tabs and vertical navigation rail.",
        samples: nav_buttons,
        kit: None,
    },
    Item {
        name: "glass::control",
        source: GLASS,
        used_in: "Outlined and quiet toolbar buttons when the theme chooses glass.",
        samples: glass_controls,
        kit: None,
    },
    Item {
        name: "overflow_toolbar::OverflowToolbar",
        source: "board-kit/dashboard/src/overflow_toolbar.rs",
        used_in: "Board toolbars, shell tabs and global actions: units that do not fit move behind a chevron.",
        samples: overflow_toolbars,
        kit: None,
    },
    Item {
        name: "surface_header::SurfaceHeader",
        source: "board-kit/dashboard/src/surface_header.rs",
        used_in: "The title bar of Logs and Search, which the shell shows in place of a board.",
        samples: surface_headers,
        kit: None,
    },
    Item {
        name: "tokens::Tokens",
        source: "board-kit/dashboard/src/tokens.rs",
        used_in: "Every Subspace colour and size: the active theme mapped to semantic tokens.",
        samples: token_swatches,
        kit: None,
    },
];

/// The retained toolbar of the overflow sample.
pub(crate) fn overflow_toolbar(cx: &mut App) -> Entity<OverflowToolbar> {
    let units = ACTIONS
        .iter()
        .map(|(id, label, icon)| {
            let (id, label, icon) = (*id, *label, *icon);
            OverflowUnit::inline(
                id,
                UNMEASURED,
                move |_, _| {
                    action_button(id, Intent::Neutral, Some(icon), label).on_click(ignore_click)
                },
                vec![PopupEntry::new(move || {
                    gpui_kit::component::menu::PopupMenuItem::new(label)
                        .icon(icon)
                        .on_click(ignore_click)
                })],
            )
        })
        .collect();
    cx.new(|_| OverflowToolbar::new(units))
}

pub(crate) fn nav_buttons(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let height = px(CAPSULE_HEIGHT);
    let capsules = DESTINATIONS.map(|(id, label, selected)| {
        glass::bleed(
            glass::nav_button(id, selected, height, tokens, cx)
                .label(label)
                .on_click(ignore_click),
        )
    });
    vec![sample(
        "Current destination and two others",
        h_flex().bg(tokens.chrome).children(capsules),
    )]
}

pub(crate) fn glass_controls(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let control = |id: &'static str, selected: bool| {
        glass::control(
            Button::new(id)
                .label("Search")
                .selected(selected)
                .on_click(ignore_click),
            selected,
            tokens,
            cx,
        )
    };
    vec![
        sample(
            "Rest",
            div()
                .p_1()
                .bg(tokens.chrome)
                .child(control("gallery-sample:glass-rest", false)),
        ),
        sample(
            "Latched",
            div()
                .p_1()
                .bg(tokens.chrome)
                .child(control("gallery-sample:glass-latched", true)),
        ),
    ]
}

pub(crate) fn overflow_toolbars(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Too narrow for five actions; open the chevron",
        sized(TOOLBAR_WIDTH, gallery.retained.toolbar.clone()),
    )]
}

pub(crate) fn surface_headers(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let header = SurfaceHeader::new(IconName::ScrollText, "Logs", "Close logs", ignore_click)
        .description("Fixture records")
        .action(
            action_button(
                "gallery-sample:header-copy",
                Intent::Neutral,
                Some(IconName::Copy),
                "Copy",
            )
            .on_click(ignore_click),
        );
    vec![sample(
        "Title, description, an action and Close",
        bounded(HEADER_BOX, tokens, header),
    )]
}

fn swatch(name: &'static str, colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .w(px(SWATCH_WIDTH))
        .gap_1()
        .child(
            div()
                .size(px(SWATCH))
                .rounded(tokens.control_radius)
                .border_1()
                .border_color(tokens.line)
                .bg(colour),
        )
        .child(div().text_size(tokens.caption_size()).child(name))
}

pub(crate) fn token_swatches(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let surfaces = [
        ("bg", tokens.bg),
        ("pane", tokens.pane),
        ("surface", tokens.surface),
        ("chrome", tokens.chrome),
        ("sidebar", tokens.sidebar),
        ("select", tokens.select),
        ("line", tokens.line),
        ("hair", tokens.hair),
    ];
    let tones = [
        ("fg", tokens.fg),
        ("fg_strong", tokens.fg_strong),
        ("muted", tokens.muted),
        ("accent", tokens.accent),
        ("red", tokens.red),
        ("orange", tokens.orange),
        ("yellow", tokens.yellow),
        ("green", tokens.green),
        ("cyan", tokens.cyan),
        ("blue", tokens.blue),
        ("magenta", tokens.magenta),
    ];
    let row = |colours: &[(&'static str, Hsla)]| {
        h_flex().flex_wrap().max_w(px(SWATCH_ROW)).gap_2().children(
            colours
                .iter()
                .map(|(name, colour)| swatch(name, *colour, tokens)),
        )
    };
    vec![
        sample("Surfaces and lines", v_flex().child(row(&surfaces))),
        sample("Text and tones", v_flex().child(row(&tones))),
    ]
}
