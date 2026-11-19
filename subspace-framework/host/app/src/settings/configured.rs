//! Narrow reconciliation of an open editor after immediate membership changes.

use super::{
    Settings, SettingsPage, background_updates, board_fields, effective_board_order,
    included_board_defaults, sources,
};
use gpui_kit::{Context, Window};

impl Settings {
    pub(crate) fn reconcile_boards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.file.rebase_web_boards() {
            self.show_error(error.to_string(), cx);
            return;
        }
        self.refresh_board_drafts(window, cx);
        self.reconcile_navigation(cx);
        self.reconcile_baseline(cx);
        cx.notify();
    }

    fn refresh_board_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let boards = self.reconciled_board_order(cx);
        self.form
            .reconcile_boards(&included_board_defaults(&boards, cx), self.file.values());
        board_fields::reconcile(&mut self.board_fields, &boards, &self.file, window, cx);
        self.reconcile_auxiliary_drafts(&boards, window, cx);
        self.board_editors
            .retain(|draft| boards.iter().any(|board| board.id == draft.id));
        let missing: Vec<_> = boards
            .iter()
            .filter(|board| !self.board_editors.iter().any(|draft| draft.id == board.id))
            .cloned()
            .collect();
        let (drafts, errors) =
            super::board_editors::load(&missing, &mut self.file, &self.form, window, cx);
        self.board_editors.extend(drafts);
        if !errors.is_empty() {
            self.show_error(errors.join("\n"), cx);
        }
        self.initial_board_order = effective_board_order(&boards, &self.file.values().board_order);
        self.boards = boards;
    }

    fn reconciled_board_order(&self, cx: &gpui_kit::App) -> Vec<crate::BoardRegistration> {
        let mut boards = crate::registry::included_registrations(cx);
        let draft_order: Vec<_> = self
            .boards
            .iter()
            .map(|board| board.id.to_owned())
            .collect();
        let order = effective_board_order(&boards, &draft_order);
        boards.sort_by_key(|board| order.iter().position(|id| id == board.id));
        boards
    }

    fn reconcile_navigation(&mut self, cx: &mut Context<Self>) {
        self.navigation.reconcile(&self.boards, &self.page, cx);
        if !self.navigation.contains(&self.page) {
            self.page = SettingsPage::General;
            self.navigation.select_general(cx);
        }
    }

    fn reconcile_baseline(&mut self, cx: &gpui_kit::App) {
        let current = self.draft_snapshot(cx);
        if let Some(initial) = &mut self.initial_draft {
            initial.reconcile_membership(&current);
        }
        if let Some(partial) = &mut self.partial_draft {
            partial.reconcile_membership(&current);
        }
    }

    fn reconcile_auxiliary_drafts(
        &mut self,
        boards: &[crate::BoardRegistration],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sources.retain(|source| {
            boards
                .iter()
                .any(|board| board.id == source.id && board.settings.notifications)
        });
        self.background_updates.retain(|draft| {
            boards
                .iter()
                .any(|board| board.id == draft.id && board.settings.background_updates)
        });
        let missing_sources: Vec<_> = boards
            .iter()
            .filter(|board| !self.sources.iter().any(|source| source.id == board.id))
            .cloned()
            .collect();
        self.sources
            .extend(sources::load(&missing_sources, window, cx));
        let missing_background: Vec<_> = boards
            .iter()
            .filter(|board| {
                !self
                    .background_updates
                    .iter()
                    .any(|draft| draft.id == board.id)
            })
            .cloned()
            .collect();
        self.background_updates
            .extend(background_updates::load(&missing_background, window, cx));
        for source in &mut self.sources {
            if let Some(board) = boards.iter().find(|board| board.id == source.id) {
                source.title = board.title.clone().into();
            }
        }
    }
}
