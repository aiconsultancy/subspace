//! Saving a draft and reviewing the server’s current version on conflict.

use gpui_kit::{AppContext as _, Context, Window};
use subspace_api_client::Error;
use subspace_api_client::decision_writes::DecisionWriteError;
use subspace_api_client::model::DecisionDetail;

use super::{Mode, Phase, Problem};
use crate::Decisions;
use crate::load::Leased;
use crate::page::{Page, PageState};

// qual:allow(coupling, oi) reason: "Asynchronous dialog writes update the retained draft, page and list together."
impl Decisions {
    pub(super) fn submit_author(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.follow_account(cx) {
            return;
        }
        let Some(editor) = self.author.as_mut() else {
            return;
        };
        if !matches!(editor.phase, Phase::Ready | Phase::Failed(_)) {
            return;
        }
        let job = match editor.job(cx) {
            Ok(job) => job,
            Err(problem) => {
                editor.phase = Phase::Failed(problem);
                cx.notify();
                return;
            }
        };
        editor.phase = Phase::Sending;
        let lease = editor.lease.clone();
        #[cfg(test)]
        let test_writer = self.test_writer.clone();
        let task = cx.background_spawn(async move {
            #[cfg(test)]
            if let Some(writer) = test_writer {
                return writer(job);
            }
            job.run(&lease)
        });
        editor.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                this.finish_author(result, window, cx)
            });
        }));
        cx.notify();
    }

    fn finish_author(
        &mut self,
        result: Result<DecisionDetail, Problem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.follow_account(cx) {
            return;
        }
        let Some(editor) = self.author.as_mut() else {
            return;
        };
        match result {
            Ok(detail) => {
                editor.phase = Phase::Ready;
                self.close_author(window, cx);
                self._page_read = None;
                self.page_reads = self.page_reads.wrapping_add(1);
                self.history = Default::default();
                let origin = self.shown_page().and_then(|page| page.origin.clone());
                self.showing_page = true;
                self.page = Some(Page {
                    origin,
                    id: detail.id.clone(),
                    title: detail.title.clone(),
                    state: PageState::Loaded(Box::new(detail)),
                });
                self.read(false, cx);
            }
            Err(Problem::Write(DecisionWriteError::Conflict)) if editor.target().is_some() => {
                self.read_current(window, cx);
            }
            Err(Problem::Write(DecisionWriteError::Unconfirmed)) => {
                editor.phase = Phase::Unconfirmed;
                self.read(false, cx);
                self.read_page(cx);
            }
            Err(problem) => editor.phase = Phase::Failed(problem),
        }
        cx.notify();
    }

    pub(super) fn read_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.author.as_mut() else {
            return;
        };
        let Some(target) = editor.target() else {
            return;
        };
        let id = target.id.clone();
        let reader = self.reader.detail.clone();
        let lease = editor.lease.clone();
        editor.phase = Phase::ReadingCurrent;
        let read = cx.background_spawn(async move { reader(&id, Some(&lease)) });
        editor.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            _ = this.update_in(cx, |this, _, cx| this.show_current(result, cx));
        }));
        cx.notify();
    }

    fn show_current(
        &mut self,
        result: Leased<Result<DecisionDetail, Error>>,
        cx: &mut Context<Self>,
    ) {
        let (lease, result) = result.split(Err);
        if !self.landed(self.author.is_some(), lease.as_ref(), cx) {
            return;
        }
        let Some(editor) = self.author.as_mut() else {
            return;
        };
        editor.phase = match result {
            Ok(detail) => Phase::Current(Box::new(detail)),
            Err(_) => Phase::ReadFailed,
        };
        cx.notify();
    }

    pub(super) fn use_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.author.as_ref() else {
            return;
        };
        let Phase::Current(detail) = &editor.phase else {
            return;
        };
        if !self.can_change(detail) {
            return;
        }
        let Some(editor) = self.author.as_mut() else {
            return;
        };
        let Phase::Current(detail) = &editor.phase else {
            return;
        };
        let Some(version) = detail.version else {
            return;
        };
        match &mut editor.mode {
            Mode::Draft {
                draft,
                target: Some(target),
            } => {
                target.version = version;
                draft.rebase_collections(detail, window, cx);
            }
            Mode::Review { target, title, .. } => {
                target.version = version;
                title.clone_from(&detail.title);
            }
            Mode::Draft { target: None, .. } => return,
        }
        editor.phase = Phase::Ready;
        cx.notify();
    }
}
