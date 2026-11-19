use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Global, KeyBinding,
    ScrollHandle, SharedString, Subscription, UniformListScrollHandle, Window, actions,
};

use super::{GridFormat, GridScope, GridSnapshot, model::Model};

mod controls;
mod interaction;
mod render;

actions!(native_grid, [CopySelection]);
struct KeysBound;
impl Global for KeysBound {}

/// A frozen export request. The application owns the native file picker and I/O.
#[derive(Clone)]
pub struct GridExport {
    snapshot: GridSnapshot,
    format: GridFormat,
    scope: GridScope,
}

impl GridExport {
    // qual:api
    // qual:allow(dry, boilerplate) reason: "Borrow the frozen result without exposing mutable export event fields across the reusable component boundary."
    pub fn snapshot(&self) -> &GridSnapshot {
        &self.snapshot
    }
    // qual:api
    // qual:allow(dry, boilerplate) reason: "Read the immutable request format without exposing mutable event fields."
    pub fn format(&self) -> GridFormat {
        self.format
    }
    // qual:api
    // qual:allow(dry, boilerplate) reason: "Read the immutable export scope without exposing mutable event fields."
    pub fn scope(&self) -> GridScope {
        self.scope
    }
}

/// Retain this entity outside render. Replace only for a newly accepted result,
/// even when its values equal the previous evaluation. Replacement clears range
/// anchors, never reassigns old identities to new records, and emits no callbacks.
pub struct NativeGrid {
    id: SharedString,
    label: SharedString,
    model: Model,
    filters: Vec<Entity<InputState>>,
    subscriptions: Vec<Subscription>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    horizontal: ScrollHandle,
    options: [bool; 8],
    options_open: bool,
    status: SharedString,
    revision: u64,
    toolbar: Entity<crate::overflow_toolbar::OverflowToolbar>,
}

impl EventEmitter<GridExport> for NativeGrid {}

impl NativeGrid {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        source: GridSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind_keys(cx);
        let mut view = Self {
            id: id.into(),
            label: label.into(),
            model: Model::new(source),
            filters: Vec::new(),
            subscriptions: Vec::new(),
            focus: cx.focus_handle().tab_stop(true),
            scroll: UniformListScrollHandle::new(),
            horizontal: ScrollHandle::new(),
            options: [true; controls::OPTION_COUNT],
            options_open: false,
            status: "".into(),
            revision: 0,
            toolbar: cx.new(|_| crate::overflow_toolbar::OverflowToolbar::new(Vec::new())),
        };
        view.build_filters(window, cx);
        view
    }

    // qual:api
    pub fn replace(&mut self, source: GridSnapshot, window: &mut Window, cx: &mut Context<Self>) {
        self.revision = self.revision.wrapping_add(1);
        let columns_changed = self.model.source.columns != source.columns;
        self.model.replace(source);
        if columns_changed {
            self.build_filters(window, cx);
        }
        self.status = "".into();
        cx.notify();
    }

    fn build_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions.clear();
        self.filters = self
            .model
            .source
            .columns
            .iter()
            .enumerate()
            .map(|(column, _)| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Contains…")
                        .default_value(self.model.filters[column].clone())
                });
                self.subscriptions
                    .push(cx.subscribe(&input, move |this, input, event, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.model.filters[column] = input.read(cx).value().to_string();
                            this.model.rebuild();
                            cx.notify();
                        }
                    }));
                input
            })
            .collect();
    }

    fn child_id(&self, suffix: impl std::fmt::Display) -> SharedString {
        format!("{}-{suffix}", self.id).into()
    }
}

fn bind_keys(cx: &mut App) {
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    cx.bind_keys([
        KeyBinding::new(
            crate::secondary!("c"),
            CopySelection,
            Some("NativeGrid && !Input"),
        ),
        KeyBinding::new("ctrl-c", CopySelection, Some("NativeGrid && !Input")),
    ]);
}
