use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{Context, Div, ParentElement as _, Styled as _, div};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

use super::super::{Installed, Manager};
use super::note;

impl Manager {
    pub(super) fn board_row(
        &self,
        board: &Installed,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        v_flex()
            .min_w_0()
            .gap_2()
            .pb_3()
            .border_b_1()
            .border_color(tokens.line)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(board.title.clone()),
                    )
                    .child(note(&board.id, tokens))
                    .children(
                        board
                            .version
                            .as_ref()
                            .map(|version| note(&format!("v{version}"), tokens)),
                    ),
            )
            .child(note(&board.source, tokens))
            .child(self.board_actions(board, cx))
            .children(
                (self.remove.as_deref() == Some(board.id.as_str()))
                    .then(|| self.removal(tokens, cx)),
            )
            .children(
                self.offer
                    .as_ref()
                    .filter(|offer| offer.id == board.id)
                    .map(|_| self.update_offer(tokens, cx)),
            )
            .children(
                self.pending
                    .as_ref()
                    .filter(|pending| pending.id == board.id && self.editor.is_none())
                    .map(|_| self.preview(tokens, cx)),
            )
    }

    fn board_actions(&self, board: &Installed, cx: &mut Context<Self>) -> Div {
        let edit_id = board.id.clone();
        let check_id = board.id.clone();
        let remove_id = board.id.clone();
        h_flex()
            .gap_2()
            .flex_wrap()
            .child(
                action_button(
                    format!("web-edit-{}", board.id),
                    Intent::Neutral,
                    Some(IconName::Pencil),
                    "Edit…",
                )
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| this.edit(&edit_id, window, cx))),
            )
            .children(board.tracked.then(|| {
                action_button(
                    format!("web-check-{}", board.id),
                    Intent::Neutral,
                    Some(IconName::RefreshCw),
                    "Check for updates",
                )
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| this.check(check_id.clone(), cx)))
            }))
            .child(
                action_button(
                    format!("web-remove-{}", board.id),
                    Intent::Quiet,
                    Some(IconName::Trash),
                    "Remove…",
                )
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.remove = Some(remove_id.clone());
                    cx.notify();
                })),
            )
    }
}
