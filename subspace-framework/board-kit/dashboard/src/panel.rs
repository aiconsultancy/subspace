//! The detail panel frame (REQ-11): a title bar (back, type and id, previous and next, the
//! service link, close), a scrolling body, and a left edge that resizes the
//! panel by dragging, by the arrow keys when it has focus (16 px, Shift for
//! 64 px), and resets on a double click. The width is the density jig's
//! `--panel-w`, clamped to 320 px and 75 % of the window, and is stored.

use std::rc::Rc;

mod density_prefs;
pub use density_prefs::save_density;
mod body;
mod toolbar;
pub use body::PanelBody;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext, BorrowAppContext as _, Div, ElementId, EmptyView, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, actions, div, px, relative,
};
pub use subspace_lucide::IconName;

use crate::AddToChat;
use crate::action::{self, Intent};
use crate::anchored::AnchoredContextMenuExt as _;
use crate::density::Density;
use crate::links;
use crate::list::{Glyph, ServiceLink};
use crate::row_menu::{self, RowMenu};
use crate::tokens::Tokens;

actions!(
    subspace_panel,
    [PanelWider, PanelNarrower, PanelMuchWider, PanelMuchNarrower]
);

pub const RESIZE_CONTEXT: &str = "SubspacePanelResize";
const WIDTH_TOKEN: &str = "--panel-w";
pub const MIN_WIDTH: f32 = 320.0;
const KEY_STEP: f32 = 16.0;
const KEY_STEP_LARGE: f32 = 64.0;
/// The share of the window the panel may take at most.
const MAX_WINDOW_SHARE: f32 = 0.75;
/// The panel never grows past its parent pane.
const FULL_PARENT_WIDTH: f32 = 1.;
/// Text size of the service link, one step under the row font.
const BAR_HEADING_SIZE_PX: f32 = 13.;
/// Horizontal padding of the title bar.
const BAR_PADDING_X_PX: f32 = 8.;
/// The resize handle sits just left of the panel's border.
const HANDLE_OFFSET_PX: f32 = -3.;
const HANDLE_WIDTH_PX: f32 = 6.;
/// How strongly the handle shows while hovered and while focused.
const HANDLE_HOVER_OPACITY: f32 = 0.4;
const HANDLE_FOCUS_OPACITY: f32 = 0.6;
/// A double click on the handle resets the width.
const RESET_CLICK_COUNT: usize = 2;

pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(RESIZE_CONTEXT);
    // The handle is on the left edge: moving it left widens the panel.
    cx.bind_keys([
        KeyBinding::new("left", PanelWider, context),
        KeyBinding::new("right", PanelNarrower, context),
        KeyBinding::new("shift-left", PanelMuchWider, context),
        KeyBinding::new("shift-right", PanelMuchNarrower, context),
    ]);
}

/// `width` limited to 320 px and 75 % of the window (`clamp(320px,
/// var(--panel-w), 75vw)`), whole pixels.
pub fn clamp_width(width: f32, window_width: f32) -> f32 {
    width
        .min(window_width * MAX_WINDOW_SHARE)
        .max(MIN_WIDTH)
        .round()
}

/// Stores a new panel width in the density jig and the state file.
fn set_width(width: f32, window: &mut Window, cx: &mut App) {
    let width = clamp_width(width, f32::from(window.viewport_size().width));
    cx.update_default_global::<Density, _>(|density, _| density.set(WIDTH_TOKEN, width));
    save_density(cx);
    window.refresh();
}

fn step_width(step: f32, window: &mut Window, cx: &mut App) {
    let current = rendered_width(window, cx);
    set_width(current + step, window, cx);
}

/// The panel's width as drawn.
pub fn rendered_width(window: &Window, cx: &App) -> f32 {
    let tokens = Tokens::current(cx);
    clamp_width(
        f32::from(tokens.panel_width),
        f32::from(window.viewport_size().width),
    )
}

/// The drag value shared by panels with a draggable left edge.
pub struct LeftResize;

/// What the panel's title bar shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelHeader {
    pub icon: Option<Glyph>,
    /// The type, such as `Task` or `Pull request`.
    pub kind: SharedString,
    pub id: SharedString,
    /// Display text instead of the opaque ID. Menus still copy the original ID.
    pub label: Option<SharedString>,
    pub link: Option<ServiceLink>,
}

pub type StepCallback = Rc<dyn Fn(isize, &mut Window, &mut App)>;

/// Close and previous/next controls for a detail panel.
pub struct PanelActions {
    close: Rc<dyn Fn(&mut Window, &mut App)>,
    step: Option<StepCallback>,
}

impl PanelActions {
    pub fn new(
        on_close: impl Fn(&mut Window, &mut App) + 'static,
        on_step: Option<StepCallback>,
    ) -> Self {
        Self {
            close: Rc::new(on_close),
            step: on_step,
        }
    }
}

/// The panel around `body`. `actions.step` adds the previous and next buttons.
pub fn frame(
    header: PanelHeader,
    body: impl Into<PanelBody>,
    actions: PanelActions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let width = rendered_width(window, cx);
    let facts = panel_facts(&header);
    let bar = toolbar::render(&tokens, header, actions, window, cx);
    let pane = panel_box(&tokens, width)
        .child(bar)
        .child(body::render(body.into()));
    resizable_left_edge(pane, "panel-resize", &tokens, window, cx)
        .anchored_context_menu(move |popup, window, cx| {
            let selected = row_menu::hold_selection(window, cx);
            row_menu::popup(popup, row_menu::panel_menu_items(&facts, &selected), None)
        })
        .into_any_element()
}

/// The panel itself: a fixed-width pane that the left edge drags wider.
fn panel_box(tokens: &Tokens, width: f32) -> Stateful<Div> {
    v_flex()
        .id("detail-panel")
        .relative()
        .flex_shrink_0()
        .w(px(width))
        // Chat reduces the dashboard's width without changing the window width.
        // Constrain the inspector to its parent so it cannot overlap that pane.
        .max_w(relative(FULL_PARENT_WIDTH))
        .h_full()
        .bg(tokens.surface)
        .border_l_1()
        .border_color(tokens.line)
}

/// Adds the shared detail-pane resize control to a positioned right-hand pane
/// that draws its width from the panel width token.
pub fn resizable_left_edge<T: InteractiveElement + ParentElement>(
    pane: T,
    id: &'static str,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut App,
) -> T {
    let focus = window
        .use_keyed_state((ElementId::from(id), "focus"), cx, |_, cx| {
            cx.focus_handle().tab_stop(true)
        })
        .read(cx)
        .clone();
    pane.child(resize_handle(id, tokens, focus, window))
        .on_drag_move::<LeftResize>(|event, window, cx| {
            let width = event.bounds.right() - event.event.position.x;
            set_width(f32::from(width), window, cx);
        })
}

/// A native quiet icon button in the title bar.
fn icon_button(
    id: &'static str,
    icon: subspace_lucide::SvgIcon,
    label: &'static str,
) -> crate::icon_button::IconButton {
    action::icon_button(id, Intent::Quiet, icon, label)
}

/// The outlined button that opens the item in its service.
pub fn service_link(id: impl Into<ElementId>, link: ServiceLink) -> impl IntoElement {
    let url = link.url;
    action::action_button(
        id,
        Intent::Neutral,
        Some(IconName::ExternalLink),
        link.label,
    )
    .on_click(move |_, _, cx| {
        links::open(&url, cx);
    })
}

/// The button that sends the open item to the selected chat engine.
fn add_to_chat_button() -> impl IntoElement {
    action::icon_button(
        "panel-add-to-chat",
        Intent::Quiet,
        IconName::MessageSquarePlus,
        "Add to chat",
    )
    .on_click(|_, window, cx| window.dispatch_action(Box::new(AddToChat), cx))
}

/// The left edge: drag it, use the arrow keys while it has focus, or double
/// click it to go back to the stored width.
fn resize_handle(
    id: &'static str,
    tokens: &Tokens,
    handle_focus: FocusHandle,
    window: &Window,
) -> impl IntoElement {
    left_resize_handle(id, tokens)
        .key_context(RESIZE_CONTEXT)
        .aria_label("Resize panel")
        .when(handle_focus.is_focused(window), |handle| {
            handle.bg(tokens.accent.opacity(HANDLE_FOCUS_OPACITY))
        })
        .test_support()
        .track_focus(&handle_focus)
        .on_action(|_: &PanelWider, window, cx| step_width(KEY_STEP, window, cx))
        .on_action(|_: &PanelNarrower, window, cx| step_width(-KEY_STEP, window, cx))
        .on_action(|_: &PanelMuchWider, window, cx| step_width(KEY_STEP_LARGE, window, cx))
        .on_action(|_: &PanelMuchNarrower, window, cx| step_width(-KEY_STEP_LARGE, window, cx))
        .on_click(move |event, window, cx| {
            if event.click_count() >= RESET_CLICK_COUNT {
                reset_width(window, cx);
            } else {
                handle_focus.focus(window, cx);
            }
        })
}

/// A canonical draggable left edge. The parent handles [`LeftResize`] moves
/// and owns width state and persistence.
pub fn left_resize_handle(id: impl Into<gpui_kit::ElementId>, tokens: &Tokens) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .left(px(HANDLE_OFFSET_PX))
        .top_0()
        .bottom_0()
        .w(px(HANDLE_WIDTH_PX))
        .cursor_col_resize()
        .hover(|style| style.bg(tokens.accent.opacity(HANDLE_HOVER_OPACITY)))
        .on_drag(LeftResize, |_, _, _, cx| cx.new(|_| EmptyView))
}

/// Drops the stored width, so the panel goes back to the default.
fn reset_width(window: &mut Window, cx: &mut App) {
    if cx.has_global::<Density>() {
        cx.global_mut::<Density>().remove(WIDTH_TOKEN);
    }
    save_density(cx);
    window.refresh();
}

/// What the panel's own menu offers: the item it shows, and its service
/// link when the title bar has one. The panel is open, so it never offers
/// to open its own details.
fn panel_facts(header: &PanelHeader) -> RowMenu {
    let facts = RowMenu::new(header.label.clone().unwrap_or_default())
        .id(header.id.clone())
        .opens_details(false);
    match &header.link {
        // The label reads "GitHub ↗"; the menu says "Open in GitHub".
        Some(link) => facts.link(link.label.trim_end_matches('↗').trim(), &link.url),
        None => facts,
    }
}

#[cfg(test)]
mod tests {
    use super::{PanelHeader, clamp_width, panel_facts};

    #[test]
    fn the_width_stays_between_320_px_and_three_quarters_of_the_window() {
        assert_eq!(clamp_width(600.0, 1280.0), 600.0);
        assert_eq!(clamp_width(1200.0, 1280.0), 960.0);
        assert_eq!(clamp_width(100.0, 1280.0), 320.0);
        // A narrow window still gets the minimum.
        assert_eq!(clamp_width(600.0, 300.0), 320.0);
        assert_eq!(clamp_width(600.4, 1280.0), 600.0);
    }

    #[test]
    fn a_friendly_header_keeps_the_real_identifier_for_copy() {
        let header = PanelHeader {
            id: "19:opaque@thread.v2".into(),
            label: Some("Delivery team".into()),
            ..Default::default()
        };
        let facts = panel_facts(&header);
        assert_eq!(facts.title, "Delivery team");
        assert_eq!(facts.id.as_deref(), Some("19:opaque@thread.v2"));
    }
}
