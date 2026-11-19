//! A session row's context menu and the confirmation a delete waits for.

use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::AlertDialog;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{IntoElement, ParentElement as _, Styled as _, WeakEntity, div};

use super::super::{ChatPanel, Context};
use super::view::RowAction;
use crate::tokens::Tokens;

/// The row's context menu, which acts on the session it was opened over.
pub(super) fn row_menu(
    id: String,
    cx: &mut Context<ChatPanel>,
) -> impl Fn(PopupMenu) -> PopupMenu + 'static {
    let panel = cx.entity().downgrade();
    move |menu| {
        let rows = [
            ("Rename", RowAction::Rename),
            ("Export…", RowAction::Export),
        ];
        let menu = rows.iter().fold(menu, |menu, (label, action)| {
            menu.item(menu_item(label, id.clone(), *action, panel.clone()))
        });
        menu.separator()
            .item(danger_item("Delete…", id.clone(), panel.clone()))
    }
}

/// The Delete entry, in the danger colour.
fn danger_item(label: &'static str, id: String, panel: WeakEntity<ChatPanel>) -> PopupMenuItem {
    let clicked = id.clone();
    let acting = panel.clone();
    PopupMenuItem::element(move |_, cx| div().text_color(Tokens::current(cx).red).child(label))
        .on_click(move |_, window, cx| {
            let id = clicked.clone();
            let _ = acting.update(cx, |panel, cx| {
                panel.row_action(RowAction::Delete, &id, window, cx)
            });
        })
}

fn menu_item(
    label: &'static str,
    id: String,
    action: RowAction,
    panel: WeakEntity<ChatPanel>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let id = id.clone();
        let _ = panel.update(cx, |panel, cx| panel.row_action(action, &id, window, cx));
    })
}

impl ChatPanel {
    /// The confirmation a delete waits for, which names the chat it removes.
    pub(in crate::agent_chat) fn render_delete_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let session = self.view.confirm_delete.clone()?;
        let title = session.title.clone();
        let confirming = cx.entity().downgrade();
        let cancelling = confirming.clone();
        Some(
            AlertDialog::new(cx)
                .confirm()
                .title("Delete this chat?")
                .description(format!(
                    "\u{201c}{title}\u{201d} and its transcript are removed from this computer. This cannot be undone."
                ))
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .cancel_text("Cancel")
                .cancel_variant(ButtonVariant::Secondary)
                // The handler reports that the dialog may close.
                .on_ok(move |_, window, cx| {
                    let _ = confirming.update(cx, |chat, cx| chat.delete_confirmed(window, cx));
                    true
                })
                .on_cancel(move |_, _, cx| {
                    let _ = cancelling.update(cx, |chat, cx| {
                        chat.view.confirm_delete = None;
                        cx.notify();
                    });
                    true
                }),
        )
    }
}
