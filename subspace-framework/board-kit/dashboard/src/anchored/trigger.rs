use super::Anchor;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{Selectable, button::Button, input::InputGroupButton};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement as _, RenderOnce, Styled as _, Window, div,
};
use std::rc::Rc;

pub(super) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// An owned native Button, shared IconButton or InputGroupButton presentation.
/// Its callbacks, retained identity and behavior stay native.
pub struct TriggerButton {
    presentation: Presentation,
}

enum Presentation {
    Button(Box<Button>),
    IconButton(Box<crate::icon_button::IconButton>),
    InputGroup(Box<InputGroupButton>),
}

// qual:allow(dry, boilerplate) reason: "This is a presentation conversion, not an error enum. Native Button rendering must keep its ID and keyed focus scope."
impl From<crate::icon_button::IconButton> for TriggerButton {
    fn from(button: crate::icon_button::IconButton) -> Self {
        Self {
            presentation: Presentation::IconButton(Box::new(button)),
        }
    }
}

impl From<Button> for TriggerButton {
    fn from(button: Button) -> Self {
        Self {
            presentation: Presentation::Button(Box::new(button)),
        }
    }
}

impl From<InputGroupButton> for TriggerButton {
    fn from(button: InputGroupButton) -> Self {
        Self {
            presentation: Presentation::InputGroup(Box::new(button)),
        }
    }
}

impl TriggerButton {
    pub(super) fn id(&mut self) -> ElementId {
        let interactivity = match &mut self.presentation {
            Presentation::Button(button) => button.interactivity(),
            Presentation::IconButton(button) => button.interactivity(),
            Presentation::InputGroup(button) => button.interactivity(),
        };
        interactivity.element_id.clone().unwrap_or(0.into())
    }

    pub(super) fn on_activate(self, handler: ClickHandler) -> Self {
        Self {
            presentation: match self.presentation {
                Presentation::Button(button) => Presentation::Button(Box::new(
                    button.on_click(move |event, window, cx| handler(event, window, cx)),
                )),
                Presentation::IconButton(button) => Presentation::IconButton(Box::new(
                    button.on_click(move |event, window, cx| handler(event, window, cx)),
                )),
                Presentation::InputGroup(button) => Presentation::InputGroup(Box::new(
                    button.on_click(move |event, window, cx| handler(event, window, cx)),
                )),
            },
        }
    }

    fn render(self, open: bool, window: &mut Window, cx: &mut App) -> AnyElement {
        match self.presentation {
            Presentation::IconButton(button) => {
                let already_open = button.is_open();
                button.open(open || already_open).render_native(window, cx)
            }
            Presentation::Button(button) => {
                let already_open = button.is_open();
                button
                    .open(open || already_open)
                    .render(window, cx)
                    .into_any_element()
            }
            Presentation::InputGroup(button) => {
                let already_open = button.is_open();
                button
                    .open(open || already_open)
                    .render(window, cx)
                    .into_any_element()
            }
        }
    }
}

/// Measure a real button and keep it visibly open while its anchor is open.
/// The button's existing callback opens the body. A repeat pointer press closes
/// before reaching that callback, even when native menu outside-dismiss runs
/// first. Keyboard/AX callbacks should check `Anchor::is_open` before opening.
pub fn trigger(anchor: &Entity<Anchor>, button: impl Into<TriggerButton>) -> Trigger {
    Trigger {
        anchor: anchor.clone(),
        button: button.into(),
    }
}

#[derive(IntoElement)]
pub struct Trigger {
    anchor: Entity<Anchor>,
    button: TriggerButton,
}

impl RenderOnce for Trigger {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.anchor.read(cx).is_open();
        let id = self.button.id();
        // Pinned Component Button and InputGroupButton use this public keyed
        // FocusHandle in their render scope. Render the native button here,
        // before adding a wrapper namespace, so focus returns to the actual
        // button (including its keyboard handler and ring), not an inert div.
        let focus = window.use_keyed_state(id, cx, |_, cx| cx.focus_handle());
        let button = self.button.render(open, window, cx);
        self.anchor.update(cx, |anchor, cx| {
            anchor.trigger_focus = focus.read(cx).clone();
        });
        let measured = self.anchor.downgrade();
        let close = self.anchor.downgrade();
        div()
            .id(("anchored-trigger", self.anchor.entity_id()))
            .relative()
            .capture_any_mouse_down(move |_, window, cx| {
                if open {
                    _ = close.update(cx, |anchor, cx| anchor.dismiss(window, cx));
                    cx.stop_propagation();
                }
            })
            .on_prepaint(move |bounds, _, cx| {
                _ = measured.update(cx, |anchor, cx| anchor.set_bounds(bounds, cx));
            })
            .child(button)
    }
}
