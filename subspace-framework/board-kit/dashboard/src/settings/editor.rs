//! A board-owned draft retained by Settings until its outer Save or Cancel.

use std::rc::Rc;

use gpui_kit::{AnyView, App, Context, Entity, Render, Subscription, Window};
use serde_json::{Map, Value};

pub type SettingsEditorFactory =
    fn(Map<String, Value>, bool, &mut Window, &mut App) -> SettingsEditor;

/// A board owns its controls and validation. The host owns persistence.
pub trait BoardSettingsEditor: Render {
    /// Includes invalid input so dirty-state tracking does not lose edits.
    fn snapshot(&self, cx: &App) -> Value;
    /// Only settings keys owned by this editor, validated before any write.
    fn values(&self, cx: &App) -> Result<Map<String, Value>, String>;
    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>);
}

type Snapshot = Rc<dyn Fn(&App) -> Value>;
type Values = Rc<dyn Fn(&App) -> Result<Map<String, Value>, String>>;
type Enabled = Rc<dyn Fn(bool, &mut App)>;
type Observer = Rc<dyn Fn(Box<dyn FnMut(&mut App)>, &mut App) -> Subscription>;

/// Type-erased view and draft readers, with no dependency on the app shell.
pub struct SettingsEditor {
    view: AnyView,
    snapshot: Snapshot,
    values: Values,
    enabled: Enabled,
    observe: Observer,
}

impl SettingsEditor {
    pub fn new<T: BoardSettingsEditor>(entity: Entity<T>) -> Self {
        let snapshot = entity.clone();
        let values = entity.clone();
        let enabled = entity.clone();
        let observed = entity.clone();
        Self {
            view: entity.into(),
            snapshot: Rc::new(move |cx| snapshot.read(cx).snapshot(cx)),
            values: Rc::new(move |cx| values.read(cx).values(cx)),
            enabled: Rc::new(move |on, cx| {
                enabled.update(cx, |editor, cx| editor.set_enabled(on, cx));
            }),
            observe: Rc::new(move |mut on_notify, cx| {
                cx.observe(&observed, move |_, cx| on_notify(cx))
            }),
        }
    }

    pub fn view(&self) -> AnyView {
        self.view.clone()
    }

    pub fn snapshot(&self, cx: &App) -> Value {
        (self.snapshot)(cx)
    }

    pub fn values(&self, cx: &App) -> Result<Map<String, Value>, String> {
        (self.values)(cx)
    }

    pub fn set_enabled(&self, enabled: bool, cx: &mut App) {
        (self.enabled)(enabled, cx);
    }

    /// Forward editor notifications while the outer owner retains the subscription.
    pub fn observe(&self, on_notify: impl FnMut(&mut App) + 'static, cx: &mut App) -> Subscription {
        (self.observe)(Box::new(on_notify), cx)
    }
}
