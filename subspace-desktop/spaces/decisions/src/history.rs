//! Immutable revisions, read on demand for the open decision.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement as _,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _,
    div, px,
};
use subspace_api_client::model::{DecisionDetail, DecisionRevision, DecisionRevisionDetail, Page};
use subspace_api_client::{Client, Error, PageRequest};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;

use crate::Decisions;
use crate::load::{self, Lease, Leased};

const PAGE_SIZE: u32 = 20;
const HISTORY_LIST_HEIGHT: f32 = 175.;

#[derive(Default)]
pub(crate) struct History {
    pub(crate) open: bool,
    items: Vec<DecisionRevision>,
    next: Option<String>,
    selected: Option<Box<DecisionRevisionDetail>>,
    requested: Option<Request>,
    problem: Option<load::Failure>,
    task: Option<Task<()>>,
}

impl History {
    pub(crate) fn snapshot(&self) -> Option<&DecisionDetail> {
        self.selected.as_ref().map(|detail| &detail.snapshot)
    }
}

#[derive(Clone)]
enum Request {
    List(Option<String>),
    Revision(String),
}

enum Reply {
    List(Page<DecisionRevision>),
    Revision(Box<DecisionRevisionDetail>),
}

impl Request {
    fn read(self, id: &str, lease: &Lease) -> Leased<Result<Reply, Error>> {
        load::leased(
            load::bound_client(Some(lease)),
            Client::account_changed,
            |client| match self {
                Request::List(cursor) => {
                    let mut page = PageRequest::default();
                    page.limit = PAGE_SIZE;
                    page.cursor = cursor;
                    client.decision_revisions(id, &page).map(Reply::List)
                }
                Request::Revision(revision) => client
                    .decision_revision(id, &revision)
                    .map(|detail| Reply::Revision(Box::new(detail))),
            },
        )
    }
}

// qual:allow(coupling, oi) reason: "History follows the displayed decision, the account lease and the window task."
impl Decisions {
    pub(crate) fn toggle_history(&mut self, cx: &mut Context<Self>) {
        if self.follow_account(cx) || !crate::author::available() {
            return;
        }
        if self.history.open {
            self.history = History::default();
            cx.notify();
        } else {
            self.history.open = true;
            self.read_history(Request::List(None), cx);
        }
    }

    fn read_history(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.follow_account(cx) || !self.history.open {
            return;
        }
        let Some(id) = self.shown_page().map(|page| page.id.clone()) else {
            return;
        };
        let Some(lease) = self.lease.clone() else {
            return;
        };
        let key = id.clone();
        self.history.requested = Some(request.clone());
        self.history.problem = None;
        let task = cx.background_spawn(async move { request.read(&id, &lease) });
        self.history.task = Some(cx.spawn(async move |this, cx| {
            let answer = task.await;
            _ = this.update(cx, |this, cx| this.finish_history(&key, answer, cx));
        }));
        cx.notify();
    }

    fn finish_history(
        &mut self,
        id: &str,
        answer: Leased<Result<Reply, Error>>,
        cx: &mut Context<Self>,
    ) {
        let (lease, result) = answer.split(Err);
        let wanted = self.history.open && self.page.as_ref().is_some_and(|page| page.id == id);
        if !self.landed(wanted, lease.as_ref(), cx) {
            return;
        }
        match result {
            Ok(Reply::List(page)) => {
                self.history.next = page.next_cursor;
                self.history.items.extend(page.items);
                self.history.requested = None;
            }
            Ok(Reply::Revision(detail)) => {
                self.history.selected = Some(detail);
                self.history.requested = None;
            }
            Err(error) => self.history.problem = Some(load::failure(error)),
        }
        cx.notify();
    }
}

pub(crate) fn render(
    history: &History,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Option<AnyElement> {
    if !history.open {
        return None;
    }
    let pending = history.requested.is_some() && history.problem.is_none();
    let mut column = v_flex()
        .gap_2()
        .child(div().text_color(tokens.fg_strong).child("Revision history"));
    column = column
        .children(read_status(history, pending, tokens, cx))
        .child(revision_list(history, pending, cx));
    if let Some(next) = history.next.clone() {
        column = column.child(
            h_flex().child(
                action_button(
                    "decision-more-revisions",
                    Intent::Neutral,
                    None,
                    "Load more revisions",
                )
                .disabled(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.read_history(Request::List(Some(next.clone())), cx)
                })),
            ),
        );
    }
    if let Some(detail) = &history.selected {
        column = column.child(snapshot(detail, tokens, cx));
    }
    Some(column.into_any_element())
}

fn read_status(
    history: &History,
    pending: bool,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Option<AnyElement> {
    let content = match history.problem {
        Some(failure) => v_flex()
            .gap_2()
            .child(feedback::empty_line(
                tokens,
                "decision-history-error",
                failure.message(),
            ))
            .children(history.requested.clone().map(|request| {
                h_flex().child(
                    action_button(
                        "decision-revision-retry",
                        Intent::Neutral,
                        Some(IconName::RotateCw),
                        "Retry",
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.read_history(request.clone(), cx)),
                    ),
                )
            })),
        None if pending => v_flex().gap_2().child(feedback::loading_line(
            tokens,
            "decision-history-loading",
            "Loading revisions…",
        )),
        None if history.items.is_empty() => v_flex().gap_2().child(feedback::empty_line(
            tokens,
            "decision-history-empty",
            "This decision has no saved revisions.",
        )),
        None => return None,
    };
    Some(content.into_any_element())
}

fn revision_list(history: &History, pending: bool, cx: &mut Context<Decisions>) -> AnyElement {
    let mut list = v_flex()
        .id("decision-revisions")
        .max_h(px(HISTORY_LIST_HEIGHT))
        .overflow_y_scroll()
        .items_start()
        .gap_1();
    for revision in &history.items {
        let id = revision.id.clone();
        let selected = history
            .selected
            .as_ref()
            .is_some_and(|detail| detail.revision.id == id);
        let date = revision
            .saved_at
            .split('T')
            .next()
            .unwrap_or(&revision.saved_at);
        let text = format!(
            "Revision {} · {} · {}",
            revision.number, date, revision.saved_by.display_name
        );
        let button = action_button(
            format!("decision-revision-{id}"),
            Intent::Neutral,
            selected.then_some(IconName::Check),
            text,
        )
        .disabled(pending)
        .on_click(
            cx.listener(move |this, _, _, cx| this.read_history(Request::Revision(id.clone()), cx)),
        );
        list = list.child(button);
    }
    list.into_any_element()
}

fn snapshot(
    detail: &DecisionRevisionDetail,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let saved = &detail.revision;
    let heading = format!(
        "Revision {} of {} · Read only",
        saved.number, detail.current_revision_number
    );
    let mut body = v_flex()
        .id("decision-revision-snapshot")
        .test_support()
        .gap_3()
        .p_3()
        .border_1()
        .border_color(tokens.line)
        .rounded(tokens.surface_radius)
        .child(div().text_color(tokens.muted).child(heading));
    if let Some(changed) = changed_fields(saved) {
        body = body.child(div().text_color(tokens.muted).child(changed));
    }
    body.children(crate::page::view::reading(&detail.snapshot, tokens, cx))
        .into_any_element()
}

fn changed_fields(saved: &DecisionRevision) -> Option<String> {
    let names = saved
        .changed_fields
        .iter()
        .map(|field| match field.as_str() {
            "created" => "Created",
            "decidedOn" => "Decided on",
            "issueIds" => "Linked issues",
            "supersedesId" | "supersededById" => "Replacement",
            _ => field,
        })
        .collect::<Vec<_>>();
    (!names.is_empty()).then(|| names.join(" · "))
}
