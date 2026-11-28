//! Literal values keep Kit's exact plain-text selection and copy behaviour.

use gpui_kit::base::{SelectableText, TextSelection, TextSelectionHandle};
use gpui_kit::{
    App, Bounds, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, RenderOnce, ScrollHandle, SharedString, StyleRefinement,
    Styled, TestSupportExt as _, Window, div,
};
use std::ops::Range;

use crate::row_menu::MenuChat;
use gpui_kit::base::StyledExt as _;

pub(crate) struct PlainState {
    pub(crate) text: SharedString,
    pub(crate) revision: u64,
    pub(crate) selection: TextSelectionHandle,
    id: ElementId,
    highlights: Vec<(Range<usize>, gpui_kit::HighlightStyle)>,
    reveal: Option<Range<usize>>,
    scroll: Option<ScrollHandle>,
}

impl PlainState {
    fn new(id: ElementId, text: SharedString, cx: &mut Context<Self>) -> Self {
        Self {
            selection: TextSelectionHandle::new(text.to_string(), cx),
            text,
            revision: 0,
            id,
            highlights: Vec::new(),
            reveal: None,
            scroll: None,
        }
    }

    pub(crate) fn set_text(&mut self, text: SharedString, cx: &mut Context<Self>) {
        if self.text != text {
            self.revision = self.revision.wrapping_add(1);
            self.selection.set_fallback_copy_text(text.to_string(), cx);
            self.text = text;
            self.highlights.clear();
            self.reveal = None;
            cx.notify();
        }
    }

    pub(crate) fn highlight(
        &mut self,
        ranges: Vec<(Range<usize>, gpui_kit::Hsla)>,
        cx: &mut Context<Self>,
    ) {
        self.highlights = ranges
            .into_iter()
            .filter(|(range, _)| self.text.get(range.clone()).is_some())
            .map(|(range, color)| {
                (
                    range,
                    gpui_kit::HighlightStyle {
                        background_color: Some(color),
                        ..Default::default()
                    },
                )
            })
            .collect();
        cx.notify();
    }
    #[cfg(test)]
    pub(crate) fn highlight_count(&self) -> usize {
        self.highlights.len()
    }
    pub(crate) fn reveal(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.reveal = self.text.get(range.clone()).map(|_| range);
        cx.notify();
    }
    pub(super) fn select_all(&self, window: &mut Window, cx: &mut App) {
        TextSelection::clear(window, cx);
        self.selection.set_local_selection(true, cx);
        window.refresh();
    }

    fn selectable_text(&self) -> SelectableText {
        SelectableText::with_handle(self.id.clone(), self.selection.clone(), self.text.clone())
    }
}

#[cfg(feature = "gpui-kit-text-patch")]
impl Render for PlainState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.selectable_text().highlights(self.highlights.clone());
        let Some(range) = self.reveal.clone() else {
            return text;
        };
        let scroll = self.scroll.clone();
        let state = cx.entity().downgrade();
        text.reveal_range(range.clone())
            .on_reveal(move |bounds, window, cx| {
                reveal_bounds(scroll.as_ref(), bounds, window);
                let state = state.clone();
                let range = range.clone();
                cx.defer(move |cx| {
                    _ = state.update(cx, |state, cx| {
                        if state.reveal.as_ref() == Some(&range) {
                            state.reveal = None;
                            // Let Kit forget this reveal before a later find
                            // command explicitly requests the same range.
                            cx.notify();
                        }
                    });
                });
            })
    }
}

#[cfg(not(feature = "gpui-kit-text-patch"))]
impl Render for PlainState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.highlights.is_empty() {
            self.reveal = None;
            return self.selectable_text().into_any_element();
        }
        let text =
            gpui_kit::StyledText::new(self.text.clone()).with_highlights(self.highlights.clone());
        if self.reveal.take().is_none() {
            return text.into_any_element();
        }
        let scroll = self.scroll.clone();
        div()
            .child(text)
            .on_children_prepainted(move |bounds, window, _| {
                let Some(bounds) = bounds.into_iter().reduce(|a, b| a.union(&b)) else {
                    return;
                };
                reveal_bounds(scroll.as_ref(), bounds, window);
            })
            .into_any_element()
    }
}

fn reveal_bounds(scroll: Option<&ScrollHandle>, bounds: Bounds<Pixels>, window: &mut Window) {
    if let Some(scroll) = scroll {
        crate::find_in_page::scope::reveal(scroll, bounds, window);
    } else {
        window.request_autoscroll(bounds);
    }
}

#[derive(IntoElement)]
pub struct PlainText {
    id: ElementId,
    text: SharedString,
    chat: MenuChat,
    style: StyleRefinement,
    menu: bool,
}

impl PlainText {
    /// Disable the default menu when the owner supplies a specific one.
    pub fn menu(mut self, enabled: bool) -> Self {
        self.menu = enabled;
        self
    }
    pub(super) fn new(id: ElementId, text: SharedString, chat: MenuChat) -> Self {
        Self {
            id,
            text,
            chat,
            style: StyleRefinement::default(),
            menu: true,
        }
    }
}

impl Styled for PlainText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for PlainText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let text = self.text.clone();
        let state: Entity<PlainState> =
            window.use_keyed_state((self.id.clone(), "plain-state"), cx, move |_, cx| {
                PlainState::new(id, text, cx)
            });
        let scope = crate::find_in_page::current(cx);
        state.update(cx, |state, cx| {
            state.scroll = scope.as_ref().and_then(|scope| scope.scroll.clone());
            state.set_text(self.text, cx);
        });
        if let Some(scope) = &scope {
            scope.session.update(cx, |session, cx| {
                session.plain(&self.id, &state, scope.identity.clone(), cx)
            });
        }
        let root = div()
            .id(ElementId::Name(format!("plain-menu-{:?}", self.id).into()))
            .test_support()
            .refine_style(&self.style)
            .child(state.clone());
        let root = if self.menu {
            super::menu::with_source(
                root,
                super::menu::Source::Plain(state),
                super::menu::Options::new(self.chat),
            )
            .into_any_element()
        } else {
            root.into_any_element()
        };
        crate::find_in_page::scope::measured(scope, self.id, root)
    }
}

#[cfg(test)]
#[path = "plain_state_tests.rs"]
mod tests;
