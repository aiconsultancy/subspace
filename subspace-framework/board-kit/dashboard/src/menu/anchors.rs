//! Owner-held trigger handles and the event wiring for a Menu body.

use super::{Menu, MenuContents, MenuEvent};
use crate::anchored::{self, Anchor};
use crate::overflow_toolbar::OverflowToolbar;
use gpui_kit::{
    AnyElement, App, AppContext as _, DismissEvent, Entity, IntoElement, SharedString,
    Subscription, WeakEntity, Window,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

struct OpenMenu {
    host: Entity<Anchor>,
    owned: bool,
    _subscriptions: Vec<Subscription>,
}

/// Retain this beside the owner's Menu option. Inline and keyboard invocation
/// use the same named anchor. Overflow passes its existing chevron anchor to
/// `open`, replacing the native menu without creating another popup host.
#[derive(Default)]
pub struct MenuAnchors {
    controls: RefCell<HashMap<SharedString, Entity<Anchor>>>,
    active: Rc<RefCell<Option<OpenMenu>>>,
    toolbar: RefCell<Option<WeakEntity<OverflowToolbar>>>,
}

impl MenuAnchors {
    pub fn anchor(&self, id: impl Into<SharedString>, cx: &mut App) -> Entity<Anchor> {
        let id = id.into();
        let inline = self.inline_anchor(id.clone(), cx);
        self.toolbar
            .borrow()
            .as_ref()
            .and_then(WeakEntity::upgrade)
            .and_then(|toolbar| toolbar.read(cx).anchor_for(&id))
            .unwrap_or(inline)
    }

    fn inline_anchor(&self, id: SharedString, cx: &mut App) -> Entity<Anchor> {
        self.controls
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| cx.new(Anchor::new))
            .clone()
    }

    /// Toolbar unit keys must match the named picker IDs. Keyboard invocation
    /// then resolves a hidden picker to the visible chevron, before it is open.
    pub fn set_toolbar(&self, toolbar: &Entity<OverflowToolbar>) {
        *self.toolbar.borrow_mut() = Some(toolbar.downgrade());
    }

    pub fn trigger(
        &self,
        id: impl Into<SharedString>,
        button: impl Into<anchored::TriggerButton>,
        cx: &mut App,
    ) -> AnyElement {
        anchored::trigger(&self.inline_anchor(id.into(), cx), button).into_any_element()
    }

    /// Returns None when a repeat keyboard/AX activation closes the menu.
    pub fn open(
        &self,
        anchor: Entity<Anchor>,
        contents: MenuContents,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<Menu>> {
        let closing = self.is_active(&anchor);
        self.dismiss(window, cx);
        (!closing).then(|| self.show(anchor, contents, window, cx))
    }

    fn is_active(&self, anchor: &Entity<Anchor>) -> bool {
        self.active
            .borrow()
            .as_ref()
            .map(|open| open.host.entity_id())
            == Some(anchor.entity_id())
    }

    fn show(
        &self,
        host: Entity<Anchor>,
        contents: MenuContents,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Menu> {
        let menu = cx.new(|cx| Menu::new(contents, window, cx));
        let close_host = host.downgrade();
        let body_subscription = window.subscribe(&menu, cx, move |menu, event, window, cx| {
            let closes = match event {
                MenuEvent::Pick(_) => !menu.read(cx).is_kept_open(),
                MenuEvent::Query(_) => false,
                MenuEvent::Close => true,
            };
            if closes {
                _ = close_host.update(cx, |host, cx| host.dismiss(window, cx));
            }
        });
        let active = Rc::downgrade(&self.active);
        let id = host.entity_id();
        let close_menu = menu.downgrade();
        let host_subscription = window.subscribe(&host, cx, move |_, _: &DismissEvent, _, cx| {
            if let Some(active) = active.upgrade()
                && active.borrow().as_ref().map(|open| open.host.entity_id()) == Some(id)
            {
                active.borrow_mut().take();
            }
            _ = close_menu.update(cx, |_, cx| cx.emit(MenuEvent::Close));
        });
        let owned = self
            .controls
            .borrow()
            .values()
            .any(|anchor| anchor == &host);
        *self.active.borrow_mut() = Some(OpenMenu {
            host: host.clone(),
            owned,
            _subscriptions: vec![body_subscription, host_subscription],
        });
        host.update(cx, |host, cx| host.show(menu.clone(), window, cx));
        menu
    }

    /// Render owned popup hosts once in the owner's root. A supplied overflow
    /// host is already rendered by its dropdown and is not rendered here again.
    pub fn view(&self) -> Option<Entity<Anchor>> {
        self.active
            .borrow()
            .as_ref()
            .filter(|open| open.owned)
            .map(|open| open.host.clone())
    }

    /// Owners must call this when hidden, deactivated or covered by another UI.
    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        if let Some(host) = self.active.borrow().as_ref().map(|open| open.host.clone()) {
            host.update(cx, |host, cx| host.dismiss(window, cx));
        }
        if let Some(toolbar) = self.toolbar.borrow().as_ref().and_then(WeakEntity::upgrade) {
            toolbar.update(cx, |toolbar, cx| toolbar.dismiss(window, cx));
        }
    }
}
