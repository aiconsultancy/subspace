//! Native selection and highlight adapters retained only for the view lifetime.
use super::state::{RegistrationKey, Session, key};
use crate::rich::plain::PlainState;
use gpui_kit::base::{RangeHighlight, RenderedText, TextViewState};
use gpui_kit::{App, Context, Entity, Subscription, WeakEntity};
use std::{ops::Range, sync::Arc};
#[derive(Clone)]
pub(super) enum Native {
    Rich(WeakEntity<TextViewState>),
    Plain(WeakEntity<PlainState>),
}
type Painted = (Snapshot, Vec<(Range<usize>, gpui_kit::Hsla)>);
pub(super) struct Entry {
    pub(super) id: Arc<str>,
    pub(super) element: gpui_kit::ElementId,
    pub(super) native: Native,
    pub(super) identity: Option<gpui_kit::SharedString>,
    pub(super) rank: usize,
    painted: std::cell::RefCell<Option<Painted>>,
    _observer: Subscription,
    _release: Subscription,
}
impl Entry {
    pub(super) fn highlight(&self, colors: Vec<(Range<usize>, gpui_kit::Hsla)>, cx: &mut App) {
        let Some(text) = self.native.snapshot(cx) else {
            return;
        };
        let current = (text, colors);
        if self.painted.borrow().as_ref() == Some(&current) {
            return;
        }
        self.native.highlight(current.1.clone(), cx);
        self.painted.replace(Some(current));
    }
    pub(super) fn new(
        element: gpui_kit::ElementId,
        registered: RegistrationKey,
        identity: Option<gpui_kit::SharedString>,
        native: Native,
        cx: &mut Context<Session>,
    ) -> Option<Self> {
        let (observer, release) = native.watch(registered, cx)?;
        Some(Self {
            id: key(&element),
            element,
            native,
            identity,
            rank: usize::MAX,
            painted: Default::default(),
            _observer: observer,
            _release: release,
        })
    }
}
fn watch<T: 'static>(
    state: &Entity<T>,
    registered: RegistrationKey,
    cx: &mut Context<Session>,
) -> (Subscription, Subscription) {
    let id = state.entity_id();
    (
        cx.observe(state, |session, _, cx| session.schedule(cx)),
        cx.observe_release(state, move |session, _, cx| {
            session.released(&registered, id, cx)
        }),
    )
}
impl Native {
    pub(super) fn id(&self) -> gpui_kit::EntityId {
        match self {
            Self::Rich(state) => state.entity_id(),
            Self::Plain(state) => state.entity_id(),
        }
    }
    fn watch(
        &self,
        registered: RegistrationKey,
        cx: &mut Context<Session>,
    ) -> Option<(Subscription, Subscription)> {
        match self {
            Self::Rich(state) => Some(watch(&state.upgrade()?, registered, cx)),
            Self::Plain(state) => Some(watch(&state.upgrade()?, registered, cx)),
        }
    }
    pub(super) fn alive(&self) -> bool {
        match self {
            Self::Rich(state) => state.upgrade().is_some(),
            Self::Plain(state) => state.upgrade().is_some(),
        }
    }
    pub(super) fn snapshot(&self, cx: &App) -> Option<Snapshot> {
        match self {
            Self::Rich(state) => Some(Snapshot::Rich(state.upgrade()?.read(cx).rendered_text())),
            Self::Plain(state) => {
                let state = state.upgrade()?;
                let state = state.read(cx);
                Some(Snapshot::Plain(state.revision, state.text.clone()))
            }
        }
    }
    pub(super) fn highlight(&self, ranges: Vec<(Range<usize>, gpui_kit::Hsla)>, cx: &mut App) {
        match self {
            Self::Rich(state) => {
                _ = state.update(cx, |state, cx| {
                    _ = state.set_range_highlights(
                        ranges
                            .into_iter()
                            .map(|(range, color)| RangeHighlight::new(range, color)),
                        cx,
                    );
                });
            }
            Self::Plain(state) => {
                _ = state.update(cx, |state, cx| state.highlight(ranges, cx));
            }
        }
    }
    pub(super) fn reveal(&self, range: Range<usize>, cx: &mut App) {
        match self {
            Self::Rich(state) => {
                _ = state.update(cx, |state, cx| {
                    _ = state.reveal_range(range, cx);
                });
            }
            Self::Plain(state) => {
                _ = state.update(cx, |state, cx| state.reveal(range, cx));
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Snapshot {
    Rich(RenderedText),
    Plain(u64, gpui_kit::SharedString),
}
impl Snapshot {
    pub(super) fn as_str(&self) -> &str {
        match self {
            Self::Rich(text) => text.as_str(),
            Self::Plain(_, text) => text.as_ref(),
        }
    }
}

impl Native {
    pub(super) fn rich(&self) -> Option<Entity<TextViewState>> {
        match self {
            Self::Rich(state) => state.upgrade(),
            Self::Plain(_) => None,
        }
    }
}
pub(super) fn compare(a: &Entry, b: &Entry, _: bool) -> std::cmp::Ordering {
    a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id))
}
