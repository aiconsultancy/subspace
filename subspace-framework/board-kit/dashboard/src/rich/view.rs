//! Retained native rich-text state with the shared text-menu policy.

use std::sync::{Arc, Mutex};

use gpui_kit::base::{TextView, TextViewState, TextViewStyle};
use gpui_kit::{
    App, Context, ElementId, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled, Subscription,
    TestSupportExt as _, WeakEntity, Window, div,
};

use crate::row_menu::MenuChat;
use gpui_kit::base::StyledExt as _;

pub(super) type Claim = Arc<dyn Fn(&str, &mut Window, &mut App) -> bool + Send + Sync>;
type Configure = Box<dyn FnOnce(TextView, RichTextContext) -> TextView>;

struct Owner {
    state: Entity<TextViewState>,
    _subscription: Subscription,
}
impl Owner {
    fn new(source: &str, supplied: Option<Entity<TextViewState>>, cx: &mut Context<Self>) -> Self {
        let state = supplied.unwrap_or_else(|| super::native_state::create(source, cx));
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            _subscription: subscription,
        }
    }
    fn adopt(&mut self, state: Entity<TextViewState>, cx: &mut Context<Self>) {
        if self.state == state {
            return;
        }
        self._subscription = cx.observe(&state, |_, _, cx| cx.notify());
        self.state = state;
        cx.notify();
    }
}

/// The native view's context for owner-supplied inline plugins. It holds a
/// weak view reference, so a plugin never retains the view which owns it.
// qual:allow(srp, god_struct) reason: "Weak plugin facade binds menu targeting, selection and find to the same native view without retaining its owner."
#[derive(Clone)]
pub struct RichTextContext {
    pub(super) state: WeakEntity<TextViewState>,
    target: Arc<Mutex<Option<String>>>,
    pub(super) fallback: Option<String>,
    highlights: Vec<(std::ops::Range<usize>, gpui_kit::Hsla)>,
}

impl RichTextContext {
    /// A more specific inline block keeps the shared native menu and view selection.
    // qual:api - Consumed by app inline renderers through the Framework facade.
    pub fn text_menu<E>(
        &self,
        element: E,
        fallback: impl Into<String>,
        chat: MenuChat,
    ) -> gpui_kit::component::menu::ContextMenu<E>
    where
        E: gpui_kit::InteractiveElement + gpui_kit::ParentElement + gpui_kit::Styled,
    {
        let mut context = self.clone();
        context.fallback = Some(fallback.into());
        super::menu::with_source(element, super::menu::Source::Rich(context), chat)
    }

    /// Find backgrounds for a custom inline label, mapped through native source positions.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn source_highlights(
        &self,
        source: std::ops::Range<usize>,
        cx: &App,
    ) -> Vec<(std::ops::Range<usize>, gpui_kit::HighlightStyle)> {
        let Some(state) = self.state.upgrade() else {
            return Vec::new();
        };
        let text = state.read(cx).rendered_text();
        let Some(label) = text.range_for_source(source) else {
            return Vec::new();
        };
        self.highlights
            .iter()
            .filter_map(|(range, color)| {
                let start = range.start.max(label.start);
                let end = range.end.min(label.end);
                (start < end).then(|| {
                    (
                        start - label.start..end - label.start,
                        gpui_kit::HighlightStyle {
                            background_color: Some(*color),
                            ..Default::default()
                        },
                    )
                })
            })
            .collect()
    }

    /// Mark a custom inline link during its secondary mouse-down, before the
    /// containing text menu opens. Ordinary Markdown links do this natively.
    // qual:api — called by app Spaces for their own inline references.
    pub fn mark_link(&self, url: impl Into<String>) {
        if let Ok(mut target) = self.target.lock() {
            *target = Some(url.into());
        }
    }

    pub(super) fn clear_link(&self) {
        if let Ok(mut target) = self.target.lock() {
            *target = None;
        }
    }

    pub(super) fn link(&self) -> Option<String> {
        self.target
            .lock()
            .ok()
            .and_then(|target| target.clone())
            .filter(|url| {
                crate::links::safe_url(url).is_some() || subspace_search::parse_link(url).is_some()
            })
    }
}

/// Shared rich text: native selection, a block-scoped menu and safe links.
/// Use `native` for additional Kit capabilities, and `menu(false)` when an
/// owner supplies a more specific context menu.
#[derive(IntoElement)]
pub struct RichText {
    id: ElementId,
    source: SharedString,
    claim: Claim,
    configure: Vec<Configure>,
    style: StyleRefinement,
    chat: MenuChat,
    menu: bool,
    fallback: Option<String>,
}

// qual:allow(dry, boilerplate) reason: "The controlled wrapper forwards native capabilities while preserving shared menu and find ownership."
impl RichText {
    pub(super) fn new(id: ElementId, source: SharedString, claim: Claim) -> Self {
        Self {
            id,
            source,
            claim,
            configure: Vec::new(),
            style: StyleRefinement::default(),
            chat: MenuChat::Offered,
            menu: true,
            fallback: None,
        }
    }

    /// Configure the real Kit view without hiding its remaining capabilities.
    pub fn native(
        mut self,
        configure: impl FnOnce(TextView, RichTextContext) -> TextView + 'static,
    ) -> Self {
        self.configure.push(Box::new(configure));
        self
    }

    pub fn style(self, style: TextViewStyle) -> Self {
        self.native(move |view, _| view.style(style))
    }

    /// Explicitly replace the default text menu with an owner's menu.
    pub fn menu(mut self, enabled: bool) -> Self {
        self.menu = enabled;
        self
    }

    // qual:allow(test_quality, untested) reason: "One-line forwarding builder; GPUI Base owns scrolling and exercises it in inline_virtual_tests, while this renderer tests its retained state and context."
    pub fn scrollable(self, enabled: bool) -> Self {
        self.native(move |view, _| view.scrollable(enabled))
    }

    pub fn chat(mut self, chat: MenuChat) -> Self {
        self.chat = chat;
        self
    }

    /// Supply the safe displayed block text when the owner already has it.
    pub fn copy_text(mut self, text: impl Into<String>) -> Self {
        self.fallback = Some(text.into());
        self
    }
}

impl Styled for RichText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for RichText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = self.source.clone();
        let scope = crate::find_in_page::current(cx);
        let pooled = scope.as_ref().and_then(|scope| {
            scope
                .session
                .read(cx)
                .pooled(&self.id, scope.identity.as_ref())
        });
        let initial = pooled.clone();
        let owner = window.use_keyed_state((self.id.clone(), "rich-owner"), cx, |_, cx| {
            Owner::new(&source, initial, cx)
        });
        if let Some(state) = pooled {
            owner.update(cx, |owner, cx| owner.adopt(state, cx));
        }
        let state = owner.read(cx).state.clone();
        if let Some(scope) = &scope {
            scope.session.update(cx, |session, cx| {
                session.rich(&self.id, &state, scope.identity.clone(), cx)
            });
        }
        state.update(cx, |state, cx| state.set_text(&self.source, cx));
        let context = RichTextContext {
            state: state.downgrade(),
            target: Arc::default(),
            fallback: self.fallback,
            highlights: scope.as_ref().map_or_else(Vec::new, |scope| {
                scope
                    .session
                    .read(cx)
                    .highlights(&self.id, scope.identity.as_ref(), cx)
            }),
        };
        let mut view = TextView::new(&state);
        if let Some(scroll) = scope.as_ref().and_then(|scope| scope.scroll.clone()) {
            view = view.on_reveal(move |bounds, window, _| {
                crate::find_in_page::scope::reveal(&scroll, bounds, window)
            });
        }
        let view = configure_view(view, self.configure, &context, self.claim);
        let clear = context.clone();
        let root = div()
            .id(ElementId::Name(format!("rich-menu-{:?}", self.id).into()))
            .test_support()
            .refine_style(&self.style)
            .capture_any_mouse_down(move |event, _, _| {
                if event.button == MouseButton::Right {
                    clear.clear_link();
                }
            })
            .child(view);
        let root = if self.menu {
            super::menu::with_source(root, super::menu::Source::Rich(context), self.chat)
                .into_any_element()
        } else {
            root.into_any_element()
        };
        crate::find_in_page::scope::measured(scope, self.id, root)
    }
}

fn configure_view(
    mut view: TextView,
    configure: Vec<Configure>,
    context: &RichTextContext,
    claim: Claim,
) -> TextView {
    view = view.selectable(true);
    for configure in configure {
        view = configure(view, context.clone());
    }
    view = view
        .on_link_click(move |url, event, window, cx| {
            if super::activates_link(event) && !claim(url, window, cx) {
                super::follow_link(url, window, cx);
            }
        })
        .image_source(|uri| super::image_source(&uri.to_string()));
    view
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
