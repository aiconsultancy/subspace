//! Native discussion editors; the hub owns their memory-only drafts.
mod attachments;
mod editor;
mod held;
mod notices;
pub(crate) mod view;
use crate::Issues;
use editor::{Editor, make_composer};
use gpui_kit::component::input::InputEvent;
use gpui_kit::{AnyWindowHandle, App, Context, FocusHandle, Window};
use std::collections::HashMap;
use subspace_api_client::hub::{CommentState, HubSnapshot, ScopeGeneration};
use subspace_framework::dashboard::composer::ComposerEvent;

pub(crate) struct AuthoringView {
    window: AnyWindowHandle,
    scope: Option<ScopeGeneration>,
    issue: Option<String>,
    root: Option<Editor>,
    reply: Option<Editor>,
    thread_focus: HashMap<String, FocusHandle>,
    discarding: Option<Option<String>>,
    intake_busy: bool,
    intake_error: Option<String>,
}

impl AuthoringView {
    pub(crate) fn clear(&mut self) {
        self.root = None;
        self.reply = None;
        self.issue = None;
        self.scope = None;
        self.discarding = None;
        self.thread_focus.clear();
        self.intake_error = None;
        self.intake_busy = false;
    }
    pub(crate) fn new(window: &Window) -> Self {
        Self {
            window: window.window_handle(),
            scope: None,
            issue: None,
            root: None,
            reply: None,
            thread_focus: HashMap::new(),
            discarding: None,
            intake_busy: false,
            intake_error: None,
        }
    }

    fn editor(&self, parent: Option<&str>) -> Option<&Editor> {
        if parent.is_none() {
            self.root.as_ref()
        } else {
            self.reply
                .as_ref()
                .filter(|e| e.parent.as_deref() == parent)
        }
    }
}

// qual:allow(coupling, oi) reason: "This lifecycle boundary follows hub scope, retires private editors, and schedules window-owned input synchronization."
impl Issues {
    pub(crate) fn follow_comments(&mut self, snapshot: &HubSnapshot, cx: &mut Context<Self>) {
        let issue = snapshot
            .issue()
            .and_then(|i| i.detail())
            .map(|i| i.fields.id.clone());
        if self.authoring.scope != Some(snapshot.generation()) || self.authoring.issue != issue {
            self.authoring.root = None;
            self.authoring.reply = None;
            self.authoring.discarding = None;
            self.authoring.thread_focus.clear();
            self.authoring.intake_error = None;
            self.authoring.intake_busy = false;
            self.authoring.scope = Some(snapshot.generation());
            self.authoring.issue = issue;
        }
        if self.authoring.discarding.as_ref().is_some_and(|parent| {
            self.hub
                .as_ref()
                .zip(self.authoring.issue.as_ref())
                .and_then(|(h, i)| h.comment_draft(i, parent.as_deref()))
                .is_none_or(|d| !matches!(d.state(), CommentState::Unclear { .. }))
        }) {
            self.authoring.discarding = None;
        }
        let (window, owner) = (self.authoring.window, self.weak.clone());
        let app: &mut App = cx;
        app.defer(move |cx| {
            _ = window.update(cx, |_, window, cx| {
                _ = owner.update(cx, |this, cx| this.sync_editors(window, cx));
            });
        });
    }

    fn new_comment_editor(
        &self,
        parent: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Editor {
        let issue = self.authoring.issue.clone().unwrap_or_default();
        let draft = self
            .hub
            .as_ref()
            .and_then(|hub| hub.comment_draft(&issue, parent.as_deref()));
        let entity = make_composer(parent.is_some(), draft.as_ref(), window, cx);
        self.install_comment_attachments(&entity, parent.clone(), cx);
        let images = self.subscribe_comment_images(&entity, parent.clone(), cx);
        let typing = self.subscribe_comment_text(&entity, parent.clone(), cx);
        let sending = self.subscribe_comment_submit(&entity, parent.clone(), window, cx);
        Editor {
            entity,
            parent,
            notice_focus: cx.focus_handle(),
            state: draft
                .as_ref()
                .map_or(CommentState::Editing, |d| d.state().clone()),
            confirmation: draft.as_ref().map_or(0, |d| d.confirmation()),
            _subscriptions: vec![typing, sending, images],
        }
    }

    fn subscribe_comment_text(
        &self,
        entity: &gpui_kit::Entity<subspace_framework::dashboard::composer::Composer>,
        parent: Option<String>,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Subscription {
        let hub = self.hub.clone();
        let issue = self.authoring.issue.clone().unwrap_or_default();
        let scope = self.authoring.scope;
        let input = entity.read(cx).input();
        let thread = parent.clone();
        cx.subscribe(&input, move |_: &mut Self, input, event, cx| {
            if matches!(event, InputEvent::Change)
                && let (Some(hub), Some(scope)) = (&hub, scope)
            {
                _ = hub.edit_comment_at(
                    scope,
                    &issue,
                    thread.as_deref(),
                    input.read(cx).value().as_str(),
                );
            }
        })
    }

    fn subscribe_comment_submit(
        &self,
        entity: &gpui_kit::Entity<subspace_framework::dashboard::composer::Composer>,
        parent: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Subscription {
        let scope = self.authoring.scope;
        let thread = parent.clone();
        let target = self.authoring.issue.clone();
        cx.subscribe_in(entity, window, move |this, _, event, window, cx| {
            if this.authoring.issue != target
                || this.authoring.scope != scope
                || this
                    .hub
                    .as_ref()
                    .is_none_or(|h| Some(h.snapshot().generation()) != scope)
            {
                return;
            }
            match event {
                ComposerEvent::Submit(text) => {
                    if let (Some(hub), Some(issue), Some(scope)) =
                        (&this.hub, &this.authoring.issue, this.authoring.scope)
                    {
                        _ = hub.edit_comment_at(scope, issue, thread.as_deref(), text);
                    }
                    this.post_comment(thread.as_deref(), false, cx);
                }
                ComposerEvent::Cancel => this.leave_comment(thread.as_deref(), window, cx),
            }
        })
    }

    fn sync_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.authoring.issue.is_none()
            || self
                .hub
                .as_ref()
                .is_none_or(|h| Some(h.snapshot().generation()) != self.authoring.scope)
        {
            return;
        }
        if self.authoring.root.is_none() {
            self.authoring.root = Some(self.new_comment_editor(None, window, cx));
        }
        if let Some(page) = self
            .issue_view
            .snapshot
            .as_ref()
            .and_then(|i| i.timeline().page())
        {
            for row in page
                .items
                .iter()
                .filter_map(|e| e.comment.as_ref())
                .filter(|c| c.parent_comment_id.is_none())
            {
                self.authoring
                    .thread_focus
                    .entry(row.id.clone())
                    .or_insert_with(|| cx.focus_handle());
            }
        }
        self.sync_editor(None, window, cx);
        if let Some(parent) = self.authoring.reply.as_ref().and_then(|e| e.parent.clone()) {
            self.sync_editor(Some(&parent), window, cx);
        }
        cx.notify();
    }

    fn sync_editor(&mut self, parent: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(hub), Some(issue)) = (&self.hub, &self.authoring.issue) else {
            return;
        };
        let Some(draft) = hub.comment_draft(issue, parent) else {
            return;
        };
        let reason = hub.comment_reason(issue, parent);
        let active = parent.is_some() || self.authoring.reply.is_none();
        let editor = if parent.is_none() {
            self.authoring.root.as_mut()
        } else {
            self.authoring.reply.as_mut()
        };
        let Some(editor) = editor else {
            return;
        };
        let failed = editor.state != *draft.state()
            && matches!(
                draft.state(),
                CommentState::Failed(_) | CommentState::Unclear { .. }
            );
        let confirmed = editor.sync(&draft, reason, window, cx);
        if active {
            editor.focus_after(failed, confirmed, window, cx);
        }
        if confirmed && let Some(parent) = parent {
            self.authoring.reply = None;
            if let Some(focus) = self.authoring.thread_focus.get(parent) {
                focus.focus(window, cx);
            }
        }
    }

    pub(crate) fn focus_comment(
        &mut self,
        parent: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if parent.is_some()
            && self
                .authoring
                .reply
                .as_ref()
                .and_then(|e| e.parent.as_ref())
                != parent.as_ref()
        {
            self.authoring.reply = Some(self.new_comment_editor(parent.clone(), window, cx));
        }
        if let Some(editor) = self.authoring.editor(parent.as_deref()) {
            editor.entity.update(cx, |e, cx| e.focus(window, cx));
        }
        cx.notify();
    }

    fn post_comment(&mut self, parent: Option<&str>, retry: bool, cx: &mut Context<Self>) {
        if let (Some(hub), Some(issue)) = (&self.hub, &self.authoring.issue) {
            _ = hub.submit_comment(issue, parent, retry);
            self.pull_comment_snapshot(cx);
        }
    }

    fn pull_comment_snapshot(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = &self.hub {
            let snapshot = hub.snapshot();
            self.follow_comments(&snapshot, cx);
        }
    }

    fn leave_comment(&mut self, parent: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = parent {
            self.authoring.reply = None;
            if let Some(focus) = self.authoring.thread_focus.get(parent) {
                focus.focus(window, cx);
            }
        } else if let Some(focus) = &self.issue_view.timeline_focus {
            focus.focus_selected(window, cx);
        }
        cx.notify();
    }
}
