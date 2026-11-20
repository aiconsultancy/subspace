use super::super::cover::cover_while_open;
use super::super::{SubspaceApp, save_prefs};
use super::{BUSY_BAR_HEIGHT, BUSY_BAR_OFFSET};
use crate::toolbar_labels::ToolbarControl;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _};
use gpui_kit::{App, Context, Div, IntoElement, SharedString, Styled as _, WeakEntity, div, px};
use subspace_dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::{Tokens, badge};

/// Shell chrome uses the same ghost-button treatment as density and settings.
pub(super) fn chrome(
    id: &'static str,
    label: &'static str,
    tooltip: &'static str,
    on: bool,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .selected(on)
        .accessibility_label(label)
        .tooltip(tooltip)
}

pub(super) fn sized_button(
    button: Button,
    icon: impl Into<Icon>,
    label: Option<SharedString>,
    loading: bool,
) -> IconButton {
    // Shell chrome is quiet; say so on the wrapper, which styles quiet toolbar
    // buttons for the theme (Liquid Glass in the macOS themes).
    let button = IconButton::from_button(button)
        .ghost()
        .icon(icon)
        .loading(loading);
    match label {
        Some(label) => button.label(label),
        None => button,
    }
}

/// Right-click toggles only this control's label preference. The menu covers
/// the board area while open.
pub(super) fn with_label_menu(
    button: IconButton,
    control: ToolbarControl,
    cx: &mut Context<SubspaceApp>,
) -> impl IntoElement {
    let shown = control.shows_label(cx);
    let app = cx.weak_entity();
    button.anchored_context_menu(move |menu, window, cx| {
        cover_while_open(&app, window, cx);
        let app = app.clone();
        menu.item(
            PopupMenuItem::new("Show label")
                .checked(shown)
                .on_click(move |_, _, cx| apply_label_toggle(control, shown, &app, cx)),
        )
    })
}

fn apply_label_toggle(
    control: ToolbarControl,
    shown: bool,
    app: &WeakEntity<SubspaceApp>,
    cx: &mut App,
) {
    control.set_label(!shown, cx);
    save_prefs(cx);
    let _ = app.update(cx, |_, cx| cx.notify());
}

/// Visible label plus the per-control context menu, after click handlers.
pub(in crate::app) fn shell_button(
    button: Button,
    control: ToolbarControl,
    icon: impl Into<Icon>,
    cx: &mut Context<SubspaceApp>,
) -> impl IntoElement {
    let label = control.shows_label(cx).then(|| control.label().into());
    with_label_menu(sized_button(button, icon, label, false), control, cx)
}

pub(super) fn busy_bar(tokens: &Tokens) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(BUSY_BAR_OFFSET))
        .h(px(BUSY_BAR_HEIGHT))
        .bg(tokens.accent)
}

/// A navigation item's count. The glass capsule is neutral, so the badge
/// keeps one accent treatment whether or not its board is current.
pub(crate) fn tab_badge(badge: SharedString, tokens: &Tokens, cx: &App) -> Div {
    badge::badge(badge, tokens.accent, tokens.bg).font_family(cx.theme().font_family.clone())
}
