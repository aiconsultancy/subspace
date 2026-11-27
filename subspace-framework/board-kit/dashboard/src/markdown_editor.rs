//! A formatted Markdown field and a highlighted source view of its draft.

#[cfg(test)]
mod tests;
mod view;

use crate::markdown_engine::{BlockEditorEvent, BlockEditorState, ImagePolicy, SubmitPolicy};
use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Focusable as _, Global, KeyBinding,
    SharedString, Subscription, Window, actions,
};

pub use view::MarkdownField;

actions!(markdown_field, [NextField, PreviousField]);

struct EditorKeys;
impl Global for EditorKeys {}

fn install_keys(cx: &mut App) {
    if cx.has_global::<EditorKeys>() {
        return;
    }
    crate::markdown_engine::init(cx);
    cx.bind_keys([
        KeyBinding::new(
            "tab",
            NextField,
            Some("SubspaceMarkdownField > BlockEditor"),
        ),
        KeyBinding::new(
            "shift-tab",
            PreviousField,
            Some("SubspaceMarkdownField > BlockEditor"),
        ),
    ]);
    cx.set_global(EditorKeys);
}

/// User actions that a form can handle.
pub enum MarkdownEditorEvent {
    /// The user edited the draft. Switching modes and loading values do not emit this.
    Changed,
    /// The user pressed the platform's modified Enter in the formatted editor.
    SubmitRequested,
}

/// Retained editor state. Form owners get and set Markdown via this entity.
pub struct MarkdownEditor {
    rich: Entity<BlockEditorState>,
    source: Entity<EditorState>,
    baseline: Imported,
    mode: Mode,
    import_failed: bool,
    placeholder: SharedString,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<MarkdownEditorEvent> for MarkdownEditor {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Markdown,
    Source,
}

/// The formatted engine canonicalises Markdown on import. Keep the original
/// bytes for an untouched draft, including when undo returns to this baseline.
#[derive(Default)]
struct Imported {
    exact: SharedString,
    canonical: SharedString,
}

impl MarkdownEditor {
    pub fn new(
        value: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        install_keys(cx);
        let value = value.into();
        let placeholder = placeholder.into();
        let rich = cx.new(|cx| {
            BlockEditorState::new(window, cx)
                .with_image_policy(ImagePolicy::Placeholder)
                .with_submit_policy(SubmitPolicy::ModifiedEnter)
        });
        let source = cx.new(|cx| source_state(value.clone(), placeholder.clone(), window, cx));
        let subscriptions = [
            cx.subscribe(&rich, |this, _, event, cx| match event {
                BlockEditorEvent::Changed { .. } if this.mode == Mode::Markdown => {
                    cx.emit(MarkdownEditorEvent::Changed);
                    cx.notify();
                }
                BlockEditorEvent::SubmitRequested if this.mode == Mode::Markdown => {
                    cx.emit(MarkdownEditorEvent::SubmitRequested);
                }
                BlockEditorEvent::Focus | BlockEditorEvent::Blur => cx.notify(),
                _ => {}
            }),
            cx.subscribe(&source, |this, _, event, cx| {
                if this.mode == Mode::Source && matches!(event, InputEvent::Change) {
                    cx.emit(MarkdownEditorEvent::Changed);
                    cx.notify();
                }
            }),
        ];
        let mut editor = Self {
            rich,
            source,
            baseline: Imported::default(),
            mode: Mode::Markdown,
            import_failed: false,
            placeholder,
            _subscriptions: subscriptions,
        };
        if !editor.load_rich(value, cx) {
            editor.mode = Mode::Source;
        }
        editor
    }

    /// The current draft, with source punctuation preserved when unedited.
    pub fn value(&self, cx: &App) -> SharedString {
        match self.mode {
            Mode::Markdown => self.rich_value(cx),
            Mode::Source => self.source.read(cx).value(),
        }
    }

    /// Load a replacement from the form owner, resetting both editing histories.
    pub fn set_value(
        &mut self,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.into();
        self.source
            .update(cx, |state, cx| state.set_value(value.clone(), window, cx));
        if !self.load_rich(value, cx) {
            self.mode = Mode::Source;
        }
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle(cx), cx);
    }

    fn focus_handle(&self, cx: &App) -> gpui_kit::FocusHandle {
        match self.mode {
            Mode::Markdown => self.rich.read(cx).focus_handle(cx),
            Mode::Source => self.source.read(cx).focus_handle(cx),
        }
    }

    fn rich_value(&self, cx: &App) -> SharedString {
        let canonical = self.rich.read(cx).source_snapshot().text().clone();
        if canonical == self.baseline.canonical {
            self.baseline.exact.clone()
        } else {
            canonical
        }
    }

    fn load_rich(&mut self, exact: SharedString, cx: &mut Context<Self>) -> bool {
        self.import_failed = self
            .rich
            .update(cx, |state, cx| state.set_source(&exact, cx))
            .is_err();
        if !self.import_failed {
            self.baseline = Imported {
                exact,
                canonical: self.rich.read(cx).source_snapshot().text().clone(),
            };
        }
        !self.import_failed
    }

    fn select(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        let value = self.value(cx);
        if mode == Mode::Markdown {
            if (self.import_failed || value != self.rich_value(cx)) && !self.load_rich(value, cx) {
                cx.notify();
                return;
            }
        } else if value != self.source.read(cx).value() {
            self.source
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        self.mode = mode;
        self.focus(window, cx);
        cx.notify();
    }
}

fn source_state(
    value: SharedString,
    placeholder: SharedString,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> EditorState {
    EditorState::new(window, cx)
        .language("markdown")
        .line_number(false)
        .folding(false)
        .indent_guides(false)
        .auto_close(false)
        .smart_indent(false)
        .placeholder(placeholder)
        .default_value(value)
}
