use std::{cell::Cell, path::PathBuf, sync::Arc};

use gpui_kit::component::{WindowExt as _, input::InputEvent, select::SelectEvent};
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, ParentElement as _, Subscription,
    Window,
};
use subspace_client_github::{
    Client,
    issue_report::{BrowserFallback, Diagnostics, IssuePaths, IssueRepository, ReportPlan},
};
use subspace_dashboard::Toast;

use crate::{attachments::Attachments, form::Form};

const DIALOG_WIDTH_REMS: f32 = 42.;

/// Where reports go: the GitHub client that files them and the repository
/// the app names.
pub struct Destination {
    pub client: Arc<Client>,
    pub repository: IssueRepository,
}

/// Retained unsent draft and one-shot submission, owned by the window shell.
pub struct ReportIssue {
    pub(super) client: Arc<Client>,
    pub(super) repository: IssueRepository,
    pub(super) diagnostics: Diagnostics,
    pub(super) available_paths: Option<IssuePaths>,
    #[cfg(feature = "test-support")]
    pub(super) save_directory: PathBuf,
    pub(super) form: Form,
    pub(super) state: State,
    pub(super) focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// The shell subscribes once and forwards these to its normal toast presenter.
#[derive(Clone, Debug)]
pub enum ReportIssueEvent {
    Toast(Toast),
}

pub(super) enum Phase {
    Editing,
    Preparing,
    Review(Box<ReportPlan>),
    Submitting,
    Created(String),
    Browser(BrowserFallback),
    Uncertain { opened: bool },
}

pub(super) struct State {
    pub phase: Phase,
    pub attachments: Attachments,
    pub next_image: usize,
    pub diagnostics_expanded: bool,
    pub body_pending: bool,
    pub image_busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub open: bool,
    pub revision: u64,
    pub fields: [String; 4],
    pub include_paths: bool,
}

impl State {
    pub fn busy(&self) -> bool {
        self.image_busy || matches!(self.phase, Phase::Preparing | Phase::Submitting)
    }

    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        if matches!(self.phase, Phase::Review(_) | Phase::Preparing) {
            self.phase = Phase::Editing;
        }
        self.error = None;
        self.notice = None;
    }
}

impl EventEmitter<ReportIssueEvent> for ReportIssue {}

impl ReportIssue {
    pub fn new(
        destination: Destination,
        diagnostics: Diagnostics,
        _save_directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let form = Form::new(window, cx);
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&form.title, Self::input_changed));
        for input in [&form.description, &form.steps] {
            subscriptions.push(cx.subscribe(input, |this, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.fields_changed(cx);
                }
            }));
        }
        subscriptions.push(cx.subscribe(&form.kind, |this, _, event, cx| {
            if matches!(event, SelectEvent::Confirm(_)) {
                this.fields_changed(cx);
            }
        }));
        Self {
            client: destination.client,
            repository: destination.repository,
            diagnostics: diagnostics.without_paths(),
            available_paths: None,
            #[cfg(feature = "test-support")]
            save_directory: _save_directory,
            form,
            focus: cx.focus_handle(),
            state: State {
                phase: Phase::Editing,
                attachments: Attachments::default(),
                next_image: 0,
                diagnostics_expanded: false,
                body_pending: false,
                image_busy: false,
                error: None,
                notice: None,
                open: false,
                revision: 0,
                include_paths: false,
                fields: [
                    String::new(),
                    String::new(),
                    String::new(),
                    "Unknown".into(),
                ],
            },
            _subscriptions: subscriptions,
        }
    }

    /// Open the existing draft. Kit owns the focus trap and Escape restoration.
    pub fn show(entity: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let Some(entity) = begin_opening(entity, window, cx) else {
            return;
        };
        let initial_focus = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            initial_focus
                .replace(false)
                .then(|| entity.focus_handle(cx).focus(window, cx));
            let closed = entity.downgrade();
            dialog
                .title("Report issue")
                .width(dialog_width(window))
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .child(entity.clone())
                .on_close(move |_, _, cx| {
                    _ = closed.update(cx, |this, _| this.state.open = false);
                })
        });
    }

    /// Refresh explicit diagnostics only while editing an unreviewed draft.
    pub fn set_diagnostics(&mut self, diagnostics: Diagnostics, cx: &mut Context<Self>) {
        if matches!(self.state.phase, Phase::Editing) && !self.state.busy() {
            self.diagnostics = diagnostics.without_paths();
            cx.notify();
        }
    }

    /// Supply sanitized path availability without opting the report into sharing it.
    /// Submitted, browser and uncertain outcomes keep their immutable reviewed text.
    pub fn set_path_diagnostics(&mut self, paths: IssuePaths, cx: &mut Context<Self>) {
        if !self.paths_mutable() {
            return;
        }
        self.available_paths = Some(paths);
        if self.state.include_paths {
            self.state.invalidate();
        }
        cx.notify();
    }

    pub(super) fn set_include_paths(&mut self, included: bool, cx: &mut Context<Self>) {
        if !self.paths_mutable()
            || (included && self.available_paths.is_none())
            || self.state.include_paths == included
        {
            return;
        }
        self.state.include_paths = included;
        self.state.invalidate();
        cx.notify();
    }

    pub(super) fn effective_diagnostics(&self) -> Diagnostics {
        let diagnostics = self.diagnostics.clone().without_paths();
        match self
            .available_paths
            .as_ref()
            .filter(|_| self.state.include_paths)
        {
            Some(paths) => diagnostics.with_paths(paths.clone()),
            None => diagnostics,
        }
    }

    fn paths_mutable(&self) -> bool {
        !self.state.image_busy
            && matches!(
                self.state.phase,
                Phase::Editing | Phase::Review(_) | Phase::Preparing
            )
    }

    fn input_changed(
        &mut self,
        _: Entity<gpui_kit::component::input::InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.fields_changed(cx);
        }
    }

    fn fields_changed(&mut self, cx: &mut Context<Self>) {
        let fields = self.form.values(cx);
        if fields == self.state.fields {
            return;
        }
        self.state.fields = fields;
        self.state.invalidate();
        cx.notify();
    }
}

fn begin_opening(
    entity: &Entity<ReportIssue>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<ReportIssue>> {
    if entity.read(cx).state.open || window.has_active_dialog(cx) {
        return None;
    }
    entity.update(cx, |this, cx| {
        if this.state.include_paths
            && matches!(
                this.state.phase,
                Phase::Editing | Phase::Review(_) | Phase::Preparing
            )
        {
            this.state.include_paths = false;
            this.state.invalidate();
        }
        this.state.open = true;
        cx.notify();
    });
    Some(entity.clone())
}

fn dialog_width(window: &Window) -> gpui_kit::Pixels {
    window.rem_size() * DIALOG_WIDTH_REMS
}

impl Focusable for ReportIssue {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.state.phase {
            Phase::Editing => self.form.description.focus_handle(cx),
            Phase::Review(_) => self.form.preview_title.focus_handle(cx),
            _ => self.focus.clone(),
        }
    }
}
