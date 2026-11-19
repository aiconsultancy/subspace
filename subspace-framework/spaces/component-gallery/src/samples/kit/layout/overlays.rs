//! GPUI Kit window layers: Root, the theme, dialogs, alert dialogs,
//! notifications, popovers, hover cards and sheets. Each overlay opens from a
//! button and closes with its own Cancel, Close or Escape.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{App, ClickEvent, Context, Hsla, ParentElement as _, Styled as _, Window, div, px};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, sample};

const SWATCH: f32 = 20.;
const SWATCH_WIDTH: f32 = 132.;
const SWATCH_ROW: f32 = 600.;
const HOVER_WIDTH: f32 = 220.;
const SHEET_TEXT: &str = "Supplementary work that needs more room than a popover.";

fn opener(
    id: &'static str,
    label: &'static str,
    open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .label(label)
        .small()
        .outline()
        .on_click(open)
}

pub(crate) fn roots(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "This window's Root shows the notification",
        opener(
            "gallery-kit:root",
            "Notify through Root",
            |_, window, cx| {
                window.push_notification(
                    Notification::info(
                        "Root hosts this window's dialogs, sheets and notifications.",
                    ),
                    cx,
                );
            },
        ),
    )]
}

fn swatch(name: &'static str, colour: Hsla, tokens: &Tokens) -> impl gpui_kit::IntoElement {
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

pub(crate) fn themes(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let theme = cx.theme();
    let colours = [
        ("background", theme.background),
        ("foreground", theme.foreground),
        ("primary", theme.primary),
        ("secondary", theme.secondary),
        ("muted", theme.muted),
        ("accent", theme.accent),
        ("border", theme.border),
        ("popover", theme.popover),
        ("sidebar", theme.sidebar),
        ("info", theme.info),
        ("success", theme.success),
        ("warning", theme.warning),
        ("danger", theme.danger),
        ("ring", theme.ring),
    ];
    vec![sample(
        "The active theme's semantic colours",
        h_flex()
            .flex_wrap()
            .max_w(px(SWATCH_ROW))
            .gap_2()
            .children(colours.map(|(name, colour)| swatch(name, colour, tokens))),
    )]
}

pub(crate) fn dialogs(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Opens a dialog with Cancel and OK",
        opener("gallery-kit:dialog", "Open dialog…", |_, window, cx| {
            window.open_dialog(cx, |dialog, _, _| {
                dialog
                    .title("Rename fixture")
                    .child("A dialog holds one focused decision or a short form.")
                    .footer(
                        DialogFooter::new()
                            .gap_2()
                            .child(
                                DialogClose::new().child(
                                    Button::new("gallery-kit:dialog-cancel")
                                        .label("Cancel")
                                        .outline(),
                                ),
                            )
                            .child(
                                DialogAction::new().child(
                                    Button::new("gallery-kit:dialog-ok")
                                        .label("Rename")
                                        .primary(),
                                ),
                            ),
                    )
            });
        }),
    )]
}

pub(crate) fn alert_dialogs(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Asks before a consequential action",
        opener(
            "gallery-kit:alert-dialog",
            "Delete fixture…",
            |_, window, cx| {
                window.open_alert_dialog(cx, |alert, _, _| {
                    alert
                        .title("Delete “Sample fixture”?")
                        .description("The fixture is removed from the gallery until it reloads.")
                        .ok_text("Delete")
                        .show_cancel(true)
                });
            },
        ),
    )]
}

pub(crate) fn notifications(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let notify = |id: &'static str, label: &'static str, note: fn() -> Notification| {
        opener(id, label, move |_, window, cx| {
            window.push_notification(note(), cx)
        })
    };
    vec![sample(
        "In-app; each button shows one type",
        h_flex()
            .gap_2()
            .child(notify("gallery-kit:notify-info", "Info", || {
                Notification::info("Fixture synced.")
            }))
            .child(notify("gallery-kit:notify-success", "Success", || {
                Notification::success("Fixture saved.")
            }))
            .child(notify("gallery-kit:notify-warning", "Warning", || {
                Notification::warning("The fixture token expires soon.")
            }))
            .child(notify("gallery-kit:notify-error", "Error", || {
                Notification::error("Couldn’t reach example.com.").title("Connection failed")
            })),
    )]
}

pub(crate) fn popovers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let button = action::action_button(
        "gallery-kit:popover",
        Intent::Neutral,
        Some(IconName::MessageSquare),
        "Show popover",
    );
    vec![sample(
        "Through anchored::Anchor; click to open",
        gallery.retained.kit.layout.popover.trigger(button),
    )]
}

pub(crate) fn hover_cards(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let muted = tokens.muted;
    vec![sample(
        "Hover the name",
        HoverCard::new("gallery-kit:hover-card")
            .trigger(div().text_color(tokens.accent).child("@ada.example"))
            .content(move |_, _, _| {
                v_flex()
                    .w(px(HOVER_WIDTH))
                    .gap_1()
                    .child(div().child("Ada Example"))
                    .child(
                        div()
                            .text_color(muted)
                            .child("Fixture person · example.com"),
                    )
            }),
    )]
}

pub(crate) fn sheets(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Slides in from the edge",
        opener("gallery-kit:sheet", "Open sheet…", |_, window, cx| {
            window.open_sheet(cx, |sheet, _, _| {
                sheet.title("Fixture details").child(SHEET_TEXT)
            });
        }),
    )]
}
