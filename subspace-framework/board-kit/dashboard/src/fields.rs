//! Canonical dashboard treatments for native text inputs and code editors.
//!
//! These constructors keep GPUI Kit's state, events, focus, and tab order
//! visible to callers. Boards continue to own field values and validation.

use gpui_kit::component::input::{Editor, EditorState, Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, Div, ElementId, Entity, IntoElement, ParentElement as _, PathPromptOptions, Pixels,
    SharedString, Styled as _, Window, div, transparent_black,
};
use subspace_lucide::IconName;

use crate::Tokens;
use crate::action::{Intent, action_button};
use crate::rich;
use crate::row_menu::MenuChat;

/// The semantic role that selects a native input's canonical size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRole {
    /// Search across board content.
    Search,
    /// A labelled configuration value.
    Configuration,
    /// A compact field in a table or dense filter row.
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSize {
    Small,
    ExtraSmall,
}

impl InputRole {
    fn size(self) -> InputSize {
        match self {
            Self::Search | Self::Configuration => InputSize::Small,
            Self::Compact => InputSize::ExtraSmall,
        }
    }
}

/// Build a native input with the dashboard treatment and an accessible name.
pub fn input(
    state: &Entity<InputState>,
    role: InputRole,
    accessible_label: impl Into<SharedString>,
) -> Input {
    let input = Input::new(state).aria_label(accessible_label.into());
    match role.size() {
        InputSize::Small => input.small(),
        InputSize::ExtraSmall => input.xsmall(),
    }
}

/// The search field of a workspace toolbar: a [`InputRole::Search`] input
/// with the search glyph and a clear button, filled with the theme's quiet
/// `pane` surface and no visible border at rest, so it sits on the toolbar
/// band like its buttons. Keyboard focus still draws the native focus ring.
pub fn toolbar_search(
    state: &Entity<InputState>,
    accessible_label: impl Into<SharedString>,
    cx: &App,
) -> Input {
    let tokens = Tokens::current(cx);
    input(state, InputRole::Search, accessible_label)
        .prefix(Icon::from(IconName::Search).xsmall())
        .cleanable(true)
        .bg(tokens.pane)
        .border_color(transparent_black())
        .rounded(tokens.control_radius)
}

/// Build a native code editor without replacing its editor state or events.
pub fn code_editor(
    state: &Entity<EditorState>,
    accessible_label: impl Into<SharedString>,
) -> Editor {
    Editor::new(state).aria_label(accessible_label.into())
}

/// A labelled multiline value in a dashboard form.
// qual:api — exercised by the Issues editor's native keyboard tests.
pub fn textarea(state: &Entity<TextareaState>, label: impl Into<SharedString>) -> Textarea {
    Textarea::new(state).aria_label(label.into())
}

/// Placement of a visible field label relative to its native input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldLayout {
    Inline { label_width: Pixels },
    Stacked,
}

/// Presentation facts for a labelled native input.
pub struct LabelledInput {
    state: Entity<InputState>,
    label: SharedString,
    role: InputRole,
    layout: FieldLayout,
    error: Option<SharedString>,
}

impl LabelledInput {
    pub fn new(
        state: &Entity<InputState>,
        label: impl Into<SharedString>,
        layout: FieldLayout,
    ) -> Self {
        Self {
            state: state.clone(),
            label: label.into(),
            role: InputRole::Configuration,
            layout,
            error: None,
        }
    }

    pub fn role(mut self, role: InputRole) -> Self {
        self.role = role;
        self
    }

    pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
        self.error = error.map(Into::into);
        self
    }
}

/// A Settings form row: the visible label above its control. The shell's
/// Settings pages and board settings editors share it.
pub fn form_field(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    v_flex().gap_1().child(label.into()).child(control)
}

/// The default size of a heading inside a Settings page.
const SETTINGS_HEADING_SIZE: f32 = 18.;

/// A heading that groups controls inside a Settings page.
pub fn heading(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    div()
        .font_family(crate::typography::heading_font(cx))
        .text_size(tokens.heading_size(SETTINGS_HEADING_SIZE))
        .child(text.into())
}

/// Muted, selectable help text under a Settings control.
pub fn note(tokens: &Tokens, message: impl Into<SharedString>) -> Div {
    note_with(tokens, message, MenuChat::Offered)
}

/// [`note`] whose text menu offers the chat only when `chat` allows it.
pub fn note_with(tokens: &Tokens, message: impl Into<SharedString>, chat: MenuChat) -> Div {
    let message = message.into();
    div()
        .text_sm()
        .text_color(tokens.muted)
        .child(rich::plain_text_with(
            format!("settings-note-{message}"),
            &message,
            chat,
        ))
}

/// A validation message under the control it describes.
pub fn field_error(tokens: &Tokens, id: impl Into<ElementId>, message: &str) -> Div {
    div()
        .text_sm()
        .text_color(tokens.red)
        .child(rich::plain_text(id, message))
}

/// A file path input with a trailing **Choose…** button that opens the
/// native file picker and writes the chosen path into the input.
pub fn path_input(
    state: &Entity<InputState>,
    accessible_label: impl Into<SharedString>,
    prompt: impl Into<SharedString>,
    disabled: bool,
) -> Div {
    let label = accessible_label.into();
    let choose = ElementId::Name(format!("choose-file-{}", state.entity_id()).into());
    h_flex()
        .w_full()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(input(state, InputRole::Configuration, label.clone()).disabled(disabled)),
        )
        .child(
            action_button(
                choose,
                Intent::Neutral,
                Some(IconName::FolderOpen),
                "Choose…",
            )
            .accessibility_label(format!("Choose {label}"))
            .disabled(disabled)
            .on_click(choose_file(state.clone(), prompt.into())),
        )
}

/// Opens the native file picker; the chosen path replaces the input's text.
fn choose_file(
    state: Entity<InputState>,
    prompt: SharedString,
) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt.clone()),
        });
        let state = state.clone();
        window
            .spawn(cx, async move |cx| {
                let Ok(Ok(Some(paths))) = paths.await else {
                    return;
                };
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                _ = state.update_in(cx, |input, window, cx| {
                    input.set_value(path.display().to_string(), window, cx);
                });
            })
            .detach();
    }
}

/// Compose a visible label, native input, and optional validation message.
pub fn labelled_input(spec: LabelledInput, tokens: &Tokens) -> Div {
    let control = input(&spec.state, spec.role, spec.label.clone());
    let error_id = format!("labelled-input-error-{}", spec.label);
    let label = div().text_color(tokens.muted).child(spec.label);
    let error = spec.error.map(|message| {
        div()
            .text_size(tokens.row_font)
            .text_color(tokens.red)
            .child(rich::plain_text(error_id, &message))
    });

    match spec.layout {
        FieldLayout::Inline { label_width } => div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label.w(label_width).flex_shrink_0())
                    .child(control),
            )
            .children(error.map(|message| message.pl(label_width))),
        FieldLayout::Stacked => div()
            .flex()
            .flex_col()
            .child(label.text_size(tokens.row_font))
            .child(control)
            .children(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldLayout, InputRole, InputSize};

    #[test]
    fn semantic_roles_select_the_expected_native_size() {
        assert_eq!(InputRole::Search.size(), InputSize::Small);
        assert_eq!(InputRole::Configuration.size(), InputSize::Small);
        assert_eq!(InputRole::Compact.size(), InputSize::ExtraSmall);
    }

    #[test]
    fn field_layouts_keep_inline_and_stacked_meaning_distinct() {
        assert_ne!(
            FieldLayout::Inline {
                label_width: gpui_kit::px(170.)
            },
            FieldLayout::Stacked
        );
    }
}
