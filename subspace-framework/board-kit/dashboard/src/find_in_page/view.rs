//! A compact native find bar above the focused Space, outside its content scope.
use super::{Provider, Session, scope};
use crate::icon_button::IconButton;
use crate::{Tokens, fields};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyView, App, AppContext as _, Context, ElementId, Entity, FocusHandle, Focusable as _, Global,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window,
    actions, div, rems,
};
use subspace_lucide::IconName;

const FIELD_WIDTH_REM: f32 = 24.;

actions!(find_in_page, [Open, Next, Previous, Close]);
#[derive(Default)]
struct Bound;
impl Global for Bound {}

pub struct FindInPage {
    child: Option<AnyView>,
    pub(super) session: Entity<Session>,
    input: Entity<InputState>,
    return_to: Option<FocusHandle>,
    subscriptions: Vec<Subscription>,
}

pub fn pane(
    id: impl Into<ElementId>,
    child: AnyView,
    window: &mut Window,
    cx: &mut App,
) -> Entity<FindInPage> {
    window.use_keyed_state((id.into(), "find-pane"), cx, |window, cx| {
        FindInPage::new(child, Provider::default(), window, cx)
    })
}

impl FindInPage {
    pub fn new(
        child: AnyView,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::create(Some(child), provider, window, cx)
    }
    fn create(
        child: Option<AnyView>,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind(cx);
        let session = cx.new(|_| Session::new(provider));
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in this Space…"));
        let input_sub = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    this.session.update(cx, |session, cx| {
                        session.query(query, cx);
                    });
                }
            },
        );
        let state_sub = cx.observe_in(&session, window, |this, _, window, cx| {
            this.session
                .update(cx, |session, cx| session.reveal(window, cx));
            cx.notify();
        });
        Self {
            child,
            session,
            input,
            return_to: None,
            subscriptions: vec![input_sub, state_sub],
        }
    }
    pub(crate) fn is_open(&self, cx: &App) -> bool {
        self.session.read(cx).open
    }
    pub fn observe_owner<T: crate::Dashboard>(
        &mut self,
        owner: &Entity<T>,
        cx: &mut Context<Self>,
    ) {
        let mut previous = owner.read(cx).find_revision(cx);
        let sub = cx.observe(owner, move |this, owner, cx| {
            let revision = owner.read(cx).find_revision(cx);
            if revision.is_some() && revision == previous {
                return;
            }
            previous = revision;
            this.session
                .update(cx, |session, cx| session.refresh_provider(cx))
        });
        self.subscriptions.push(sub);
    }
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.read(cx).open {
            self.return_to = window.focused(cx);
            self.session.update(cx, |session, cx| session.show(cx));
        }
        self.input.focus_handle(cx).focus(window, cx);
        window.refresh();
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| session.close(cx));
        if let Some(focus) = self.return_to.take() {
            focus.focus(window, cx);
        }
        window.refresh();
        cx.notify();
    }
    fn step(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.step(backwards, window, cx));
    }
    fn bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let (at, total) = self.session.read(cx).cursor.position();
        let count = format!("{at} of {total}");
        h_flex()
            .id("find-in-page-bar")
            .test_support()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(tokens.hair)
            .child(
                div()
                    .flex_1()
                    .max_w(rems(FIELD_WIDTH_REM))
                    .child(fields::input(
                        &self.input,
                        fields::InputRole::Search,
                        "Find in this Space",
                    )),
            )
            .child(
                div()
                    .id("find-match-count")
                    .test_support()
                    .aria_label(format!("Match {count}"))
                    .text_color(tokens.muted)
                    .child(count),
            )
            .child(
                IconButton::new("find-previous")
                    .icon(IconName::ChevronUp)
                    .aria_label("Previous match")
                    .disabled(total == 0)
                    .on_click(cx.listener(|this, _, window, cx| this.step(true, window, cx))),
            )
            .child(
                IconButton::new("find-next")
                    .icon(IconName::ChevronDown)
                    .aria_label("Next match")
                    .disabled(total == 0)
                    .on_click(cx.listener(|this, _, window, cx| this.step(false, window, cx))),
            )
            .child(
                IconButton::new("find-close")
                    .icon(IconName::X)
                    .aria_label("Close find")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
    }
}
impl FindInPage {
    fn frame(
        &mut self,
        child: gpui_kit::AnyElement,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let open = self.session.read(cx).open;
        v_flex()
            .id("find-in-page")
            .key_context("FindInPage")
            .size_full()
            .min_h_0()
            .min_w_0()
            .on_action(cx.listener(|this, _: &Open, window, cx| this.show(window, cx)))
            .on_action(cx.listener(|this, _: &Next, window, cx| this.step(false, window, cx)))
            .on_action(cx.listener(|this, _: &Previous, window, cx| this.step(true, window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if this.session.read(cx).open {
                    this.close(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .capture_key_down(cx.listener(
                move |this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    if !this.session.read(cx).open
                        || !this.input.focus_handle(cx).is_focused(window)
                    {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "enter" if this.input.focus_handle(cx).is_focused(window) => {
                            this.step(event.keystroke.modifiers.shift, window, cx);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                },
            ))
            .children(open.then(|| self.bar(cx)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(scope::wrap(self.session.clone(), child)),
            )
            .into_any_element()
    }
}
impl Render for FindInPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let child = self
            .child
            .clone()
            .map_or_else(|| div().into_any_element(), IntoElement::into_any_element);
        self.frame(child, cx)
    }
}
/// A find-enabled element composition, for shared surfaces such as Help.
pub fn surface(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    _window: &mut Window,
    _cx: &mut App,
) -> gpui_kit::AnyElement {
    Surface {
        id: id.into(),
        child: child.into_any_element(),
    }
    .into_any_element()
}
#[derive(IntoElement)]
struct Surface {
    id: ElementId,
    child: gpui_kit::AnyElement,
}
impl gpui_kit::RenderOnce for Surface {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id, "find-surface"), cx, |window, cx| {
            FindInPage::create(None, Provider::default(), window, cx)
        });
        state.update(cx, |state, cx| state.frame(self.child, cx))
    }
}
fn bind(cx: &mut App) {
    if cx.try_global::<Bound>().is_some() {
        return;
    }
    cx.set_global(Bound);
    cx.bind_keys([
        KeyBinding::new(
            "escape",
            Close,
            Some("FindInPage && !SubspaceRawKeys && !SubspaceModal"),
        ),
        KeyBinding::new(
            crate::secondary!("f"),
            Open,
            Some("FindInPage && !SubspaceRawKeys && !SubspaceModal"),
        ),
        KeyBinding::new(
            crate::secondary!("g"),
            Next,
            Some("FindInPage && !SubspaceRawKeys && !SubspaceModal"),
        ),
        KeyBinding::new(
            crate::secondary!("shift-g"),
            Previous,
            Some("FindInPage && !SubspaceRawKeys && !SubspaceModal"),
        ),
    ]);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
