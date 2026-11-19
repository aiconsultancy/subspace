//! The shared boundary for trigger-anchored controls and native popup adapters.
//!
//! Retain an `Entity<Anchor>`, wrap its button with [`trigger`], and render the
//! anchor as a child. [`Anchor::show`] hosts a focusable body without a second
//! popup. The body owns its selection/search model; the anchor owns placement,
//! dismissal and focus. Owners can subscribe to `DismissEvent` to clear their
//! own `Option<Entity<Menu>>` after the host closes.

mod control;
mod floating;
mod surface;
mod trigger;

pub use control::{Dropdown, dropdown_menu};
pub use floating::floating;
pub use trigger::{TriggerButton, trigger};

use gpui_kit::base::{DeferredPopover, GlobalState};
use gpui_kit::component::color_picker::{ColorPickerState, ColorSelect};
use gpui_kit::component::menu::{ContextMenu, ContextMenuExt as _, PopupMenu};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::select::{Select, SelectDelegate, SelectItem, SelectState};
use gpui_kit::{
    AnyView, App, Bounds, Context, DismissEvent, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Pixels, Render, Styled,
    Subscription, Window,
};

/// Delegate semantic selection, search and placement to the pinned native Select.
// qual:api
pub fn select<D>(state: &Entity<SelectState<D>>) -> Select<D>
where
    D: SelectDelegate + 'static,
    <D::Item as SelectItem>::Value: PartialEq + Clone,
{
    Select::new(state)
}

/// A colour form field whose picker popover GPUI Kit hosts and places, as
/// [`select`] does for a native Select.
pub fn color_select(state: &Entity<ColorPickerState>) -> ColorSelect {
    ColorSelect::new(state)
}

/// A native popover for forms with nested pickers. Their menus may extend past
/// the form, so close the form through its trigger, Escape or a close control.
pub fn form_popover(id: impl Into<ElementId>) -> Popover {
    Popover::new(id).overlay_closable(false)
}

/// Invocation-point context menus remain native, including submenu behavior.
pub trait AnchoredContextMenuExt: InteractiveElement + ParentElement + Styled + Sized {
    #[track_caller]
    fn anchored_context_menu(
        self,
        builder: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> ContextMenu<Self> {
        self.context_menu(builder)
    }
}

impl<T: InteractiveElement + ParentElement + Styled> AnchoredContextMenuExt for T {}

/// An open body and the resources that exist only while it is shown.
struct Content {
    view: AnyView,
    focus: FocusHandle,
    registration: DeferredPopover,
    dismissal: Dismissal,
}

/// Who closes the body on Escape and outside presses.
enum Dismissal {
    /// The anchor's surface handles them.
    Surface,
    /// A native PopupMenu handles them and emits `DismissEvent`.
    NativeMenu { _subscription: Subscription },
}

/// Retained popup lifecycle and measured trigger. Render this entity beside the
/// trigger, not inside a second DropdownMenu or Popover.
pub struct Anchor {
    bounds: Bounds<Pixels>,
    trigger_focus: FocusHandle,
    surface_focus: FocusHandle,
    content: Option<Content>,
}

impl Anchor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            bounds: Bounds::default(),
            trigger_focus: cx.focus_handle(),
            surface_focus: cx.focus_handle(),
            content: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.content.is_some()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    /// Supply measured bounds for a keyboard/overflow invocation. Ordinary
    /// buttons use [`trigger`] instead. Coordinates are window-relative.
    pub fn set_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.bounds != bounds {
            self.bounds = bounds;
            cx.notify();
        }
    }

    /// Open a body and focus its existing entry handle. Opening an already-open
    /// anchor replaces the body; use `is_open`/`dismiss` for explicit toggling.
    pub fn show<T>(&mut self, body: Entity<T>, window: &mut Window, cx: &mut Context<Self>)
    where
        T: Render + Focusable + 'static,
    {
        // A replaced native menu must not dismiss the new body.
        if let Some(content) = &mut self.content {
            content.dismissal = Dismissal::Surface;
        }
        let focus = body.focus_handle(cx);
        // Register the new body before the replaced one releases its own.
        self.content = Some(Content {
            view: body.into(),
            focus: focus.clone(),
            registration: GlobalState::register_deferred_popover(cx),
            dismissal: Dismissal::Surface,
        });
        focus.focus(window, cx);
        cx.notify();
    }

    /// Close first, then emit `DismissEvent`. Restore the trigger only while
    /// focus still belongs to this popup; an action's new focus is preserved.
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Content {
            view: _view,
            focus,
            registration,
            dismissal,
        }) = self.content.take()
        else {
            return;
        };
        if self.surface_focus.contains_focused(window, cx) || focus.contains_focused(window, cx) {
            self.trigger_focus.focus(window, cx);
        }
        drop(registration);
        drop(dismissal);
        cx.emit(DismissEvent);
        cx.notify();
    }

    /// Replace this anchor's body with a native menu, including focus and its
    /// dismissal subscription. Overflow entries use their existing host here.
    /// Application callers use [`Self::show_menu_with`] so native menu
    /// construction and action-context ownership remain in this boundary.
    fn show_menu(&mut self, body: Entity<PopupMenu>, window: &mut Window, cx: &mut Context<Self>) {
        self.show(body.clone(), window, cx);
        let anchor = cx.weak_entity();
        if let Some(content) = &mut self.content {
            let subscription = window.subscribe(&body, cx, move |_, _, window, cx| {
                _ = anchor.update(cx, |anchor, cx| anchor.dismiss(window, cx));
            });
            content.dismissal = Dismissal::NativeMenu {
                _subscription: subscription,
            };
        }
    }

    /// Build and show a native menu on this retained host. The menu dispatches
    /// actions from the actual trigger and keeps the existing dismissal owner.
    pub fn show_menu_with(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
    ) {
        let focus = self.trigger_focus.clone();
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            build(menu.action_context(focus), window, cx)
        });
        self.show_menu(menu, window, cx);
    }
}

impl EventEmitter<DismissEvent> for Anchor {}

impl Focusable for Anchor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.trigger_focus.clone()
    }
}

impl Render for Anchor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        surface::render(self, cx)
    }
}

#[cfg(test)]
mod runtime_tests;
