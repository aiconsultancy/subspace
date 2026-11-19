//! Keep native navigation while refreshing callbacks from the current owner.

use super::{OverflowAnchor, OverflowUnit};
use gpui_kit::IntoElement as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Action, AnyElement, App, ClickEvent, EntityId, SharedString, Window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type Renderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type Key = (SharedString, usize);

#[derive(Default)]
struct Current {
    handler: Option<Handler>,
    render: Option<Renderer>,
}

#[derive(PartialEq)]
enum Appearance {
    Item(SharedString, bool, bool, bool, bool, bool),
    Element(bool, bool, bool, bool),
    Submenu(SharedString, bool, bool, EntityId),
    Label(SharedString),
    Separator,
}

struct Signature {
    key: Key,
    appearance: Appearance,
    action: Option<Box<dyn Action>>,
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.appearance == other.appearance
            && match (&self.action, &other.action) {
                (None, None) => true,
                (Some(left), Some(right)) => left.partial_eq(right.as_ref()),
                _ => false,
            }
    }
}

#[derive(Default)]
pub(super) struct Projection {
    entries: HashMap<Key, Rc<RefCell<Current>>>,
    signature: Vec<Signature>,
}

impl Projection {
    pub(super) fn build(
        &mut self,
        units: &[OverflowUnit],
        anchor: OverflowAnchor,
    ) -> (bool, Vec<PopupMenuItem>) {
        let mut signature = Vec::new();
        let items = units
            .iter()
            .flat_map(|unit| {
                unit.popup_entries
                    .iter()
                    .enumerate()
                    .map(|(ix, entry)| ((unit.key.clone(), ix), entry.build(anchor.clone())))
            })
            .map(|(key, item)| {
                let current = self.entries.entry(key.clone()).or_default().clone();
                let (appearance, action, item) = project(item, current);
                signature.push(Signature {
                    key,
                    appearance,
                    action,
                });
                item
            })
            .collect();
        let changed = self.signature != signature;
        self.entries
            .retain(|key, _| signature.iter().any(|entry| &entry.key == key));
        self.signature = signature;
        (changed, items)
    }
}

fn handler(current: &Rc<RefCell<Current>>) -> Handler {
    let current = current.clone();
    Rc::new(move |event, window, cx| {
        let callback = current.borrow().handler.clone();
        if let Some(callback) = callback {
            callback(event, window, cx);
        }
    })
}

fn renderer(current: &Rc<RefCell<Current>>) -> Box<dyn Fn(&mut Window, &mut App) -> AnyElement> {
    let current = current.clone();
    Box::new(move |window, cx| {
        let render = current.borrow().render.clone();
        render.map_or_else(
            || gpui_kit::div().into_any_element(),
            |render| render(window, cx),
        )
    })
}

fn project(
    mut item: PopupMenuItem,
    current: Rc<RefCell<Current>>,
) -> (Appearance, Option<Box<dyn Action>>, PopupMenuItem) {
    let (appearance, action) = appearance(&item);
    let state = take_current(&mut item, renderer(&current));
    let proxy = state.handler.is_some();
    *current.borrow_mut() = state;
    let callback = handler(&current);
    let item = item.when(proxy, |item| {
        item.on_click(move |event, window, cx| callback(event, window, cx))
    });
    (appearance, action, item)
}

fn take_current(
    item: &mut PopupMenuItem,
    proxy: Box<dyn Fn(&mut Window, &mut App) -> AnyElement>,
) -> Current {
    let render = match item {
        PopupMenuItem::ElementItem { render, .. } => {
            Some(Rc::from(std::mem::replace(render, proxy)))
        }
        _ => None,
    };
    match item {
        PopupMenuItem::Item { handler, .. } | PopupMenuItem::ElementItem { handler, .. } => {
            Current {
                handler: handler.take(),
                render,
            }
        }
        _ => Current::default(),
    }
}

fn appearance(item: &PopupMenuItem) -> (Appearance, Option<Box<dyn Action>>) {
    match item {
        PopupMenuItem::Item {
            icon,
            label,
            disabled,
            checked,
            is_link,
            action,
            handler: callback,
        } => (
            Appearance::Item(
                label.clone(),
                *disabled,
                *checked,
                *is_link,
                icon.is_some(),
                callback.is_some(),
            ),
            action.as_ref().map(|action| action.boxed_clone()),
        ),
        PopupMenuItem::ElementItem {
            icon,
            disabled,
            checked,
            action,
            handler: callback,
            ..
        } => (
            Appearance::Element(*disabled, *checked, icon.is_some(), callback.is_some()),
            action.as_ref().map(|action| action.boxed_clone()),
        ),
        PopupMenuItem::Submenu {
            icon,
            label,
            disabled,
            menu,
        } => (
            Appearance::Submenu(label.clone(), *disabled, icon.is_some(), menu.entity_id()),
            None,
        ),
        PopupMenuItem::Label(label) => (Appearance::Label(label.clone()), None),
        PopupMenuItem::Separator => (Appearance::Separator, None),
    }
}
