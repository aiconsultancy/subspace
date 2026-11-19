//! The Markdown composer for comments, reviews and Markdown fields: a
//! text area with Write and Preview, `secondary-enter` submits, and Escape
//! asks once before it discards changed text.
//!
//! A dashboard keeps the composer in an `Option<Entity<Composer>>`, renders
//! it (in its detail panel, say) and subscribes to [`ComposerEvent`]. The
//! setters change the label, placeholder, submit label, text and rules of a
//! composer in place, so a mode switch keeps the typed text.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    App, AppContext, Context, Entity, EntityInputHandler as _, EventEmitter, FocusHandle,
    KeyBinding, SharedString, Subscription, Window, actions,
};

actions!(subspace_composer, [ComposerSubmit, ComposerEscape]);

pub const COMPOSER_CONTEXT: &str = "SubspaceComposer";

/// Smallest and largest number of text rows the text area grows to.
const INPUT_MIN_ROWS: usize = 4;
const INPUT_MAX_ROWS: usize = 20;
/// Vertical padding of a Write/Preview tab.
const TAB_PADDING_Y_PX: f32 = 2.;
/// Least height of the preview box, so a short preview does not jump.
const PREVIEW_MIN_HEIGHT_PX: f32 = 96.;
const MAX_IMAGES: usize = 8;
const MAX_IMAGE_DRAFT_BYTES: usize = 32 * 1024 * 1024;

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        // In the text area, ahead of its own Enter and Escape bindings.
        KeyBinding::new(
            crate::secondary!("enter"),
            ComposerSubmit,
            Some("SubspaceComposer > Input"),
        ),
        KeyBinding::new("escape", ComposerEscape, Some("SubspaceComposer > Input")),
        // In preview, where the composer itself has focus.
        KeyBinding::new(
            crate::secondary!("enter"),
            ComposerSubmit,
            Some(COMPOSER_CONTEXT),
        ),
        KeyBinding::new("escape", ComposerEscape, Some(COMPOSER_CONTEXT)),
    ]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerEvent {
    /// The text: not empty after trimming, unless the composer is optional.
    Submit(String),
    /// The user closed the composer; any text is discarded.
    Cancel,
}

/// What Escape does: close at once when there is nothing to lose (the text
/// is empty or as it started) or the user already confirmed, else ask first.
pub fn escape_closes(text: &str, initial: &str, asked: bool) -> bool {
    text.trim().is_empty() || text == initial || asked
}

/// Why `text` cannot be submitted, if it cannot.
pub fn submit_error(text: &str, optional: bool) -> Option<String> {
    (!optional && text.trim().is_empty()).then(|| "Text is required.".into())
}

/// The hint under the text area, which reports the state the composer is in
/// and the keys that leave it.
pub fn note_text(busy: bool, asked: bool, optional: bool, submit_label: &str) -> String {
    if busy {
        return "Sending…".to_owned();
    }
    if asked {
        return "Press Esc again to discard the changes.".to_owned();
    }
    let keys = format!(
        "Ctrl+Enter: {} · Esc to cancel",
        submit_label.to_lowercase()
    );
    if optional {
        format!("Text is optional · {keys}")
    } else {
        keys
    }
}

pub struct Composer {
    images: ComposerImages,
    state: ComposerState,
    hint: Option<SharedString>,
    label: SharedString,
    submit_label: SharedString,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    /// The text the composer started with; Escape leaves it without asking.
    initial: String,
    preview: bool,
    error: Option<SharedString>,
    _subscription: Subscription,
}

struct ComposerState {
    retained: bool,
    readonly: bool,
    submit_enabled: bool,
    /// Empty text may be submitted, such as an approval without a comment.
    optional: bool,
    /// Escape was pressed once with changed text in the composer.
    asked: bool,
    busy: bool,
}

type ComposerAccessory = dyn Fn(&mut Window, &mut App) -> gpui_kit::AnyElement;

#[derive(Default)]
struct ComposerImages {
    enabled: bool,
    byte_limit: Option<usize>,
    accessory: Option<std::sync::Arc<ComposerAccessory>>,
}

impl EventEmitter<ComposerEvent> for Composer {}
pub struct ComposerImagePaste(pub crate::chat_input::attachments::IngestionBatch);
impl EventEmitter<ComposerImagePaste> for Composer {}

impl Composer {
    pub fn new(
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.into();
        let input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx).placeholder(placeholder);
            state.set_auto_grow(INPUT_MIN_ROWS, INPUT_MAX_ROWS, cx);
            state
        });
        let subscription = cx.subscribe(&input, |composer: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::Change)
                && (composer.state.asked || composer.error.is_some())
            {
                composer.state.asked = false;
                composer.error = None;
                cx.notify();
            }
        });
        Self {
            images: ComposerImages::default(),
            state: ComposerState {
                retained: false,
                readonly: false,
                submit_enabled: true,
                optional: false,
                asked: false,
                busy: false,
            },
            hint: None,
            label: label.into(),
            submit_label: "Submit".into(),
            input,
            focus: cx.focus_handle(),
            initial: String::new(),
            preview: false,
            error: None,
            _subscription: subscription,
        }
    }

    /// Start from `text`, such as a field's stored Markdown. Escape leaves
    /// without asking while the text is unchanged.
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.initial = text.to_owned();
        let text = text.to_owned();
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = label.into();
        cx.notify();
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    /// The submit button's label, such as "Save" or "Approve".
    // qual:api
    pub fn set_submit_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.submit_label = label.into();
        cx.notify();
    }

    /// Whether empty text may be submitted.
    // qual:api
    pub fn set_optional(&mut self, optional: bool, cx: &mut Context<Self>) {
        self.state.optional = optional;
        self.error = None;
        cx.notify();
    }

    /// Let an owner receive bounded clipboard images and copied paths independently from text paste.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn set_images(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.images.enabled = enabled;
        cx.notify();
    }

    /// Override the per-image clipboard bound when a service supports another limit.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn set_image_byte_limit(&mut self, bytes: usize, cx: &mut Context<Self>) {
        self.images.byte_limit = Some(bytes);
        cx.notify();
    }

    /// Owner controls after the editor and before the commit actions.
    // qual:api
    pub fn set_accessory(
        &mut self,
        render: impl Fn(&mut Window, &mut App) -> gpui_kit::AnyElement + 'static,
        cx: &mut Context<Self>,
    ) {
        self.images.accessory = Some(std::sync::Arc::new(render));
        cx.notify();
    }

    /// Focus the text area.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Retained drafts leave the editor on Escape without discarding text.
    // qual:api
    pub fn set_retained(&mut self, retained: bool, cx: &mut Context<Self>) {
        self.state.retained = retained;
        cx.notify();
    }

    /// The text state for an owner's memory-only draft subscription.
    pub fn input(&self) -> Entity<TextareaState> {
        self.input.clone()
    }

    /// Frozen text remains selectable; submit and editing are held separately.
    // qual:api
    pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.state.readonly = readonly;
        cx.notify();
    }

    // qual:api
    pub fn set_submit_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.state.submit_enabled = enabled;
        cx.notify();
    }

    /// Describe an owner's held state when the normal submit shortcut is unavailable.
    // qual:api
    pub fn set_hint(&mut self, hint: Option<SharedString>, cx: &mut Context<Self>) {
        self.hint = hint;
        cx.notify();
    }

    /// Show `error` under the text, such as a failed send.
    pub fn set_error(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        self.state.busy = false;
        cx.notify();
    }

    /// While busy, the composer shows that it is sending and ignores submits.
    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.state.busy = busy;
        cx.notify();
    }

    fn submit(&mut self, _: &ComposerSubmit, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.busy
            || self.state.readonly
            || !self.state.submit_enabled
            || self.input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        {
            return;
        }
        let text = self.text(cx);
        if let Some(error) = submit_error(&text, self.state.optional) {
            self.error = Some(error.into());
            cx.notify();
            return;
        }
        cx.emit(ComposerEvent::Submit(text));
    }

    fn escape(&mut self, _: &ComposerEscape, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.retained || escape_closes(&self.text(cx), &self.initial, self.state.asked) {
            cx.emit(ComposerEvent::Cancel);
        } else {
            self.state.asked = true;
            cx.notify();
        }
    }

    fn set_preview(&mut self, preview: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.preview = preview;
        if preview {
            self.focus.focus(window, cx);
        } else {
            self.focus(window, cx);
        }
        cx.notify();
    }
}

mod view;

#[cfg(test)]
#[path = "composer/tests.rs"]
mod retained_tests;

#[cfg(test)]
mod tests {
    use super::{escape_closes, note_text, submit_error};

    #[test]
    fn escape_asks_once_before_discarding_changes() {
        assert!(escape_closes("", "", false));
        assert!(escape_closes("  \n", "", false));
        assert!(!escape_closes("draft", "", false));
        assert!(escape_closes("draft", "", true));
        assert!(escape_closes("stored", "stored", false), "unchanged text");
        assert!(!escape_closes("stored!", "stored", false));
    }

    #[test]
    fn submit_rules() {
        assert_eq!(
            submit_error(" ", false).as_deref(),
            Some("Text is required.")
        );
        assert_eq!(submit_error("text", false), None);
        assert_eq!(submit_error("", true), None, "an optional composer");
    }

    #[test]
    fn note_reports_the_composer_state() {
        assert_eq!(note_text(true, true, true, "Save"), "Sending…");
        assert_eq!(
            note_text(false, true, false, "Save"),
            "Press Esc again to discard the changes."
        );
        assert_eq!(
            note_text(false, false, false, "Save"),
            "Ctrl+Enter: save · Esc to cancel"
        );
        assert_eq!(
            note_text(false, false, true, "Approve"),
            "Text is optional · Ctrl+Enter: approve · Esc to cancel"
        );
    }
}
