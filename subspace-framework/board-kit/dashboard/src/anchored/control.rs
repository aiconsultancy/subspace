use super::{Anchor, TriggerButton, trigger};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{
    App, Context, ElementId, Entity, IntoElement, ParentElement as _, RenderOnce, Window, div,
};
use std::rc::Rc;

type MenuBuilder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// A native menu body in the shared flipping/toggling host. Button styling stays
/// with the caller (normally `action::action_button` or `action::menu_button`).
pub fn dropdown_menu(
    button: impl Into<TriggerButton>,
    builder: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
) -> Dropdown {
    let mut button = button.into();
    let id = button.id();
    Dropdown {
        id,
        button,
        builder: Rc::new(builder),
        anchor: None,
    }
}

#[derive(IntoElement)]
pub struct Dropdown {
    id: ElementId,
    button: TriggerButton,
    builder: MenuBuilder,
    anchor: Option<Entity<Anchor>>,
}

impl Dropdown {
    /// Share a measured trigger/lifecycle with a chooser opened from overflow.
    /// Replacing the native body with `Anchor::show` keeps the chevron as the
    /// focus return target and needs no board-local positioning arithmetic.
    pub fn with_anchor(mut self, anchor: &Entity<Anchor>) -> Self {
        self.anchor = Some(anchor.clone());
        self
    }
}

impl RenderOnce for Dropdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let anchor = self.anchor.unwrap_or_else(|| {
            window.use_keyed_state((self.id, "anchored"), cx, |_, cx| Anchor::new(cx))
        });
        let owner = anchor.downgrade();
        let button = self.button.on_activate(Rc::new(move |_, window, cx| {
            _ = owner.update(cx, |anchor, cx| {
                if anchor.is_open() {
                    anchor.dismiss(window, cx);
                    return;
                }
                let focus = anchor.trigger_focus.clone();
                let menu = PopupMenu::build(window, cx, |menu, window, cx| {
                    (self.builder)(menu.action_context(focus), window, cx)
                });
                anchor.show_menu(menu, window, cx);
            });
        }));
        div().child(trigger(&anchor, button)).child(anchor)
    }
}
