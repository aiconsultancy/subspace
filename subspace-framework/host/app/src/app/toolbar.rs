//! The shell's toolbar: the dashboard switcher, the dashboard's own
//! controls, and the shell's chrome buttons.
mod chrome;
mod globals;
mod layout;
mod menu;
mod reorder;
pub(super) mod style;
#[cfg(test)]
pub(in crate::app) mod tests;
use layout::group_fits_beside_tabs;
use style::{busy_bar, shell_button, sized_button, tab_badge, with_label_menu};

use crate::toolbar_labels::ToolbarControl;
use gpui_kit::TestSupportExt as _;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, Styled, WeakEntity, Window, div, px, svg,
};
use subspace_dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_dashboard::{PaletteMode, Tokens};
use subspace_dashboard::{glass, measure};
use subspace_lucide::IconName;

use super::keys::{chat_key, palette_key};
use super::navigation::NavigationMode;
use super::space_menu::with_space_menu;
use super::{SubspaceApp, ToggleChat};

/// Toolbar layout, in pixels.
const LOGO_SIZE: f32 = 14.;
/// The first row's least height, with or without the logo.
const ROW_MIN_HEIGHT: f32 = 28.;
/// A tab capsule's padding around the label's line; `--nav-h` is its least height.
const TAB_VERTICAL_PADDING: f32 = 10.;
/// Clear space above and below a tab capsule inside the title bar.
const TAB_ROW_INSET: f32 = 5.;
const TAB_ROW_CLEARANCE: f32 = TAB_ROW_INSET * 2.;
pub(super) const TAB_ICON_SIZE: f32 = 18.;
const TAB_FONT: f32 = 15.;
const BAR_GAP: f32 = 10.;
const BAR_PAD_RIGHT: f32 = 8.;
const GALLERY_TAB_OPACITY: f32 = 0.5;
/// The busy underline sits one pixel below the toolbar's bottom border.
const BUSY_BAR_OFFSET: f32 = -1.;
const BUSY_BAR_HEIGHT: f32 = 2.;
/// A tab's width guess before GPUI reports its real measured size.
const TAB_WIDTH_GUESS: f32 = 110.;
/// Retain enough first-row space to open the dashboard overflow menu.
const MIN_SWITCHER_WIDTH: f32 = 28.;

/// One switcher tab's fixed-at-build-time presentation data, carried into
/// its overflow unit's inline and popup closures.
struct TabData {
    id: &'static str,
    title: SharedString,
    description: SharedString,
    icon: Option<&'static str>,
    badge: Option<SharedString>,
    active: bool,
    /// Whether the tab's menu can close its Space: not the last one.
    closable: bool,
}

impl SubspaceApp {
    /// One switcher tab, built fresh for every frame from `data`: a capsule
    /// that is glass while its board is current.
    fn dashboard_tab_button(data: &TabData, tokens: Tokens, cx: &App) -> Button {
        let tab_font = tokens.tab_size(TAB_FONT);
        let icon_color = glass::label_color(data.active, &tokens);
        glass::nav_button(
            format!("space-tab-{}", data.id),
            data.active,
            tab_height(&tokens),
            &tokens,
            cx,
        )
        .flex_shrink_0()
        .accessibility_label(data.description.clone())
        .tooltip(data.description.clone())
        .when_some(data.icon, |button, path| {
            button.child(
                svg()
                    .path(path)
                    .size(px(TAB_ICON_SIZE))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
        })
        .child(
            div()
                .font_family(subspace_dashboard::typography::heading_font(cx))
                .font_weight(FontWeight::MEDIUM)
                .text_size(tab_font)
                .child(data.title.clone()),
        )
        .when_some(data.badge.clone(), |button, badge| {
            button.child(tab_badge(badge, &tokens, cx))
        })
    }

    /// One dashboard's overflow unit: an inline tab and a checked popup entry
    /// that switch to the same dashboard, so the chevron menu and the
    /// visible tab share one activation path.
    fn tab_unit(data: TabData, tokens: Tokens, weak: WeakEntity<Self>) -> OverflowUnit {
        let id = data.id;
        let title: SharedString = data.badge.as_ref().map_or_else(
            || data.title.clone(),
            |badge| format!("{} · {badge}", data.title).into(),
        );
        let icon = data.icon;
        let active = data.active;
        let click_weak = weak.clone();
        let drag_weak = weak.clone();
        let menu_weak = weak.clone();
        let inline = move |_: &mut Window, cx: &mut App| {
            let click_weak = click_weak.clone();
            let button =
                Self::dashboard_tab_button(&data, tokens, cx).on_click(move |_, window, cx| {
                    _ = click_weak.update(cx, |app, cx| app.switch_to_tab(id, window, cx));
                });
            let tab = reorder::draggable_tab(button, &data, tokens, drag_weak.clone());
            with_space_menu(tab, id, data.closable, menu_weak.clone())
        };
        let popup_weak = weak;
        let popup = PopupEntry::new(move || {
            let popup_weak = popup_weak.clone();
            PopupMenuItem::new(title.clone())
                .when_some(icon, |item, path| {
                    item.icon(gpui_kit::component::Icon::default().path(path))
                })
                .checked(active)
                .on_click(move |_, window, cx| {
                    _ = popup_weak.update(cx, |app, cx| app.switch_to_tab(id, window, cx));
                })
        });
        OverflowUnit::inline(
            format!("dashboard-tab-{id}"),
            TAB_WIDTH_GUESS,
            inline,
            vec![popup],
        )
    }

    fn switch_to_tab(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.dashboards.iter().position(|board| board.id == id) {
            self.switch_to(index, window, cx);
        }
    }

    /// Rebuilds the tabs' overflow units for this frame. Rebuilding every
    /// render keeps tab state (active, badge) current; `set_units` keeps
    /// their remembered measured widths.
    fn sync_tab_units(&mut self, tokens: Tokens, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let closable = self.can_close_space();
        let mut units: Vec<OverflowUnit> = self
            .dashboards
            .iter()
            .enumerate()
            .map(|(index, dashboard)| {
                let data = TabData {
                    id: dashboard.id,
                    title: dashboard.title.clone(),
                    description: dashboard.description.clone(),
                    icon: dashboard.icon,
                    badge: dashboard.badge(cx),
                    active: self.shows_current(index),
                    closable,
                };
                Self::tab_unit(data, tokens, weak.clone())
            })
            .collect();
        // The plus follows the last tab, and is the first unit to overflow.
        units.push(super::add_space::menu::tab_unit(weak));
        self.tabs_toolbar
            .update(cx, |toolbar, cx| toolbar.set_units(units, cx));
    }

    /// The tab strip, constrained to the space its flex parent allocates so
    /// the shared overflow row measures a real viewport.
    fn tabs_region(&self, gallery: bool) -> impl IntoElement + use<> {
        div()
            .id("dashboard-tabs")
            .test_support()
            .flex_grow_1()
            .flex_shrink_1()
            .min_w(px(MIN_SWITCHER_WIDTH))
            .max_w_full()
            .when(gallery, |tabs| tabs.opacity(GALLERY_TAB_OPACITY))
            .child(self.tabs_toolbar.clone())
    }

    /// The search affordance that opens the command palette.
    pub(super) fn palette_button(cx: &mut Context<Self>) -> impl IntoElement {
        let control = ToolbarControl::Commands;
        shell_button(
            Button::new("open-palette")
                .ghost()
                .small()
                .accessibility_label(control.label())
                .tooltip(format!("Commands ({})", palette_key()))
                .on_click(cx.listener(|app, _, window, cx| {
                    app.open_palette(PaletteMode::Commands, window, cx);
                })),
            control,
            IconName::Command,
            cx,
        )
    }

    /// The selected chat engine's toggle, with the panel's own indicator.
    pub(super) fn chat_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let control = ToolbarControl::Chat;
        let chat = self.chat.read(cx);
        let indicator = chat.indicator();
        let button = Button::new("toggle-chat")
            .ghost()
            .small()
            .selected(self.chat_open)
            .accessibility_label(control.label())
            .tooltip(format!("{indicator} ({})", chat_key()))
            .on_click(cx.listener(|app, _, window, cx| app.toggle_chat(&ToggleChat, window, cx)));
        let icon = chat.engine_icon();
        let label = control.shows_label(cx).then(|| indicator.into());
        let button = sized_button(button, icon, label, false);
        with_label_menu(button, control, cx)
    }

    /// Shows or hides the docked terminal.
    pub(super) fn terminal_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let control = ToolbarControl::Terminal;
        let button = Button::new("toggle-terminal")
            .ghost()
            .small()
            .selected(self.shown_terminal().is_some())
            .accessibility_label(control.label())
            .tooltip(format!("Terminal ({})", crate::terminal::toggle_key()))
            .on_click(cx.listener(|app, _, window, cx| {
                app.change_terminal(super::terminal_dock::Change::Toggle, window, cx);
            }));
        shell_button(button, control, IconName::SquareTerminal, cx)
    }

    fn gallery_label(tokens: &Tokens) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .gap_1()
            .text_color(tokens.accent)
            .child(IconName::Maximize)
            .child("Canvas gallery")
    }

    /// Whether the active dashboard is loading, and its own controls.
    pub(super) fn toolbar_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (bool, Option<AnyElement>) {
        if !self.main.is_board() {
            return (false, None);
        }
        match self.dashboards.get(self.active) {
            Some(dashboard) => (dashboard.busy(cx), dashboard.render_toolbar(window, cx)),
            None => (false, None),
        }
    }

    fn observe_row_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if measure::changed(self.toolbar_row_width, width) {
            self.toolbar_row_width = width;
            cx.notify();
        }
    }

    fn observe_group_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if measure::changed(self.toolbar_group_width, width) {
            self.toolbar_group_width = width;
            cx.notify();
        }
    }

    /// An `on_prepaint` closure that feeds a measured width into `record`
    /// through a weak handle, so the probe itself holds no entity state.
    fn width_probe(
        weak: WeakEntity<Self>,
        record: fn(&mut Self, f32, &mut Context<Self>),
    ) -> impl Fn(gpui_kit::Bounds<gpui_kit::Pixels>, &mut Window, &mut App) + 'static {
        move |bounds, _, cx| {
            _ = weak.update(cx, |app, cx| record(app, f32::from(bounds.size.width), cx));
        }
    }

    /// The first row's middle region, between the logo and global actions.
    fn toolbar_middle(
        &self,
        show_tabs: bool,
        gallery: bool,
        group_element: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let probe = Self::width_probe(cx.weak_entity(), Self::observe_row_width);
        h_flex()
            .id("app-toolbar-row")
            .test_support()
            .flex_1()
            .min_w(px(MIN_SWITCHER_WIDTH))
            .overflow_hidden()
            .items_center()
            .gap(px(BAR_GAP))
            .on_prepaint(probe)
            .when(show_tabs, |bar| bar.child(self.tabs_region(gallery)))
            .children(group_element)
    }

    fn global_region(&self, cx: &App) -> impl IntoElement + use<> {
        div()
            .id("global-toolbar")
            .test_support()
            .flex_shrink_1()
            .min_w(subspace_dashboard::icon_button::Metrics::current(cx).overflow_width())
            .min_h(px(ROW_MIN_HEIGHT))
            .max_w_full()
            .ml_auto()
            .flex()
            .items_center()
            .child(self.globals_toolbar.clone())
    }

    /// The title-bar row: the logo, the middle region and the global actions
    /// on one centre line with the traffic lights. Both navigation modes use
    /// the same row, so switching modes never moves the board.
    fn first_row(
        &mut self,
        tokens: &Tokens,
        children: [AnyElement; 3],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let controls = subspace_dashboard::icon_button::Metrics::current(cx)
            .row_height()
            .to_pixels(window.rem_size());
        let height = first_row_height(tab_height(tokens), tokens, controls);
        super::titlebar::align_traffic_lights(&mut self.traffic_lights_row, height, window);
        h_flex()
            .id("app-toolbar-first-row")
            .test_support()
            .pl(super::titlebar::leading_inset(window, tokens.pad_x))
            .items_center()
            .h(height)
            .gap(px(BAR_GAP))
            .children(children)
            .into_any_element()
    }

    // qual:allow(iosp) reason: "One GPUI row composes tabs, the board group, gallery state, and fixed shell actions, with the responsive beside/below placement decided from measured widths."
    pub(super) fn render_toolbar(
        &mut self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (busy, controls) = self.toolbar_state(window, cx);
        let gallery = gallery_visible(self.chat_open, self.chat.read(cx).canvas_gallery());
        self.sync_global_units(busy, cx);
        let shell_actions = self.global_region(cx);

        let show_tabs = self.navigation_mode == NavigationMode::Horizontal;
        if show_tabs {
            self.sync_tab_units(*tokens, cx);
        }
        let tabs_width = if show_tabs {
            self.tabs_toolbar.read(cx).content_width()
        } else {
            measure::UNMEASURED
        };
        let has_group = controls.is_some() && !gallery;
        let beside = gallery
            || !show_tabs
            || !has_group
            || group_fits_beside_tabs(self.toolbar_row_width, tabs_width, self.toolbar_group_width);
        let group_element = select_group(
            gallery,
            Self::gallery_label(tokens).into_any_element(),
            measured_group(
                controls,
                Self::width_probe(cx.weak_entity(), Self::observe_group_width),
            ),
        );
        let (beside_group, below_group) = if beside {
            (group_element, None)
        } else {
            (None, group_element)
        };
        let middle = self.toolbar_middle(show_tabs, gallery, beside_group, cx);
        let children = [
            toolbar_logo(busy, cx),
            middle.into_any_element(),
            shell_actions.into_any_element(),
        ];
        let first_row = self.first_row(tokens, children, window, cx);
        let busy_element = busy.then(|| busy_bar(tokens).into_any_element());
        let bar = super::titlebar::draggable(v_flex().id("app-toolbar"), window, cx);
        toolbar_view(bar, tokens, first_row, below_group, busy_element)
    }
}

fn toolbar_logo(busy: bool, cx: &App) -> AnyElement {
    use subspace_dashboard::activity_mark::{ActivityMark, in_product_toolbar};
    let size = px(LOGO_SIZE);
    let artwork = if busy && in_product_toolbar(cx) {
        ActivityMark::new("toolbar-activity-mark", size).into_any_element()
    } else {
        crate::assets::product_logo(cx, size)
    };
    div()
        .id("toolbar-logo")
        .test_support()
        .flex_shrink_0()
        .child(artwork)
        .into_any_element()
}

fn tab_height(tokens: &Tokens) -> gpui_kit::Pixels {
    let line = f32::from(tokens.tab_size(TAB_FONT)) + TAB_VERTICAL_PADDING;
    px(f32::from(tokens.controls.nav_height).max(line))
}

/// The title bar's first row: the theme's chrome height, grown to hold a tab
/// capsule with clear space above and below, or the global controls.
fn first_row_height(
    tab: gpui_kit::Pixels,
    tokens: &Tokens,
    controls: gpui_kit::Pixels,
) -> gpui_kit::Pixels {
    (tab + px(TAB_ROW_CLEARANCE))
        .max(tokens.chrome_height)
        .max(controls)
}

fn gallery_visible(chat_open: bool, gallery: bool) -> bool {
    chat_open && gallery
}

fn toolbar_view(
    bar: gpui_kit::Stateful<gpui_kit::Div>,
    tokens: &Tokens,
    first_row: AnyElement,
    below_group: Option<AnyElement>,
    busy_element: Option<AnyElement>,
) -> impl IntoElement {
    bar.test_support()
        .relative()
        .w_full()
        .flex_shrink_0()
        .gap(px(BAR_GAP))
        .pl(tokens.pad_x)
        .pr(px(BAR_PAD_RIGHT))
        .bg(tokens.chrome)
        .border_b_1()
        .border_color(tokens.chrome_line)
        .text_size(tokens.row_font)
        .child(first_row)
        .children(below_group.map(|group| {
            h_flex()
                .id("board-toolbar-row")
                .test_support()
                .min_w_0()
                .max_w_full()
                .pl(px(LOGO_SIZE + BAR_GAP))
                .child(group)
        }))
        .children(busy_element)
}

fn measured_group(
    controls: Option<AnyElement>,
    probe: impl Fn(gpui_kit::Bounds<gpui_kit::Pixels>, &mut Window, &mut App) + 'static,
) -> Option<AnyElement> {
    controls.map(|controls| {
        div()
            .id("dashboard-toolbar")
            .flex_shrink_0()
            .max_w_full()
            .child(controls)
            .on_prepaint(probe)
            .test_support()
            .into_any_element()
    })
}

fn select_group(
    gallery: bool,
    label: AnyElement,
    board_group: Option<AnyElement>,
) -> Option<AnyElement> {
    if gallery { Some(label) } else { board_group }
}
