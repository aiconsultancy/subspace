use crate::markdown_engine::{BlockEditor, BlockEditorState, EditorToolbar};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{IndentInline, OutdentInline};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, DefiniteLength, Div, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, rems,
};

use super::{MarkdownEditor, Mode, NextField, PreviousField};
use crate::segmented::{self, Segment};
use crate::{Tokens, fields};

const INSET_X_REMS: f32 = 0.75;
const INSET_Y_REMS: f32 = 0.5;

/// The labelled form field for a retained [MarkdownEditor].
#[derive(IntoElement)]
pub struct MarkdownField {
    state: Entity<MarkdownEditor>,
    mode_id: &'static str,
    label: SharedString,
    height: DefiniteLength,
    readonly: bool,
}

impl MarkdownField {
    pub fn new(
        state: &Entity<MarkdownEditor>,
        mode_id: &'static str,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            state: state.clone(),
            mode_id,
            label: label.into(),
            height: rems(14.).into(),
            readonly: false,
        }
    }

    pub fn h(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = height.into();
        self
    }

    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    fn header(&self, mode: Mode, tokens: &Tokens) -> Div {
        let state = self.state.downgrade();
        h_flex()
            .gap_2()
            .flex_wrap()
            .justify_between()
            .child(self.label.clone())
            .child(segmented::control(
                self.mode_id,
                [Segment::new("Markdown"), Segment::new("Source")],
                Some(usize::from(mode == Mode::Source)),
                tokens,
                move |index, _, window, cx| {
                    let mode = if index == 0 {
                        Mode::Markdown
                    } else {
                        Mode::Source
                    };
                    _ = state.update(cx, |editor, cx| editor.select(mode, window, cx));
                },
            ))
    }

    fn body(&self, tokens: &Tokens, window: &Window, cx: &App) -> AnyElement {
        let state = self.state.read(cx);
        match state.mode {
            Mode::Source => source(state, self),
            Mode::Markdown => formatted(state, self, tokens, window, cx),
        }
    }
}

impl RenderOnce for MarkdownField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let state = self.state.read(cx);
        let mode = state.mode;
        let import_failed = state.import_failed;
        let rich = state.rich.clone();
        if rich.read(cx).is_readonly() != self.readonly {
            rich.update(cx, |editor, cx| {
                BlockEditorState::set_readonly(editor, self.readonly, cx)
            });
        }
        v_flex()
            .gap_2()
            .min_w_0()
            .child(self.header(mode, &tokens))
            .when(import_failed, |field| {
                field.child(fields::note(&tokens, "This draft can be edited in Source."))
            })
            .child(self.body(&tokens, window, cx))
    }
}

fn source(state: &MarkdownEditor, field: &MarkdownField) -> AnyElement {
    div()
        .id(SharedString::from(format!("{}:source", field.mode_id)))
        .test_support()
        .capture_action(|_: &IndentInline, window, cx| {
            cx.stop_propagation();
            window.focus_next(cx);
        })
        .capture_action(|_: &OutdentInline, window, cx| {
            cx.stop_propagation();
            window.focus_prev(cx);
        })
        .child(
            fields::code_editor(&state.source, field.label.clone())
                .h(field.height)
                .readonly(field.readonly),
        )
        .into_any_element()
}

fn formatted(
    state: &MarkdownEditor,
    field: &MarkdownField,
    tokens: &Tokens,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let rich = state.rich.clone();
    let placeholder = state.placeholder.clone();
    let empty = state.rich_value(cx).is_empty();
    let focus = rich.read(cx).focus_handle(cx);
    let scroll = rich.read(cx).scroll_handle();
    let outline = if focus.is_focused(window) {
        cx.theme().ring
    } else {
        tokens.line
    };
    v_flex()
        .id(SharedString::from(format!("{}:formatted", field.mode_id)))
        .test_support()
        .role(Role::Group)
        .aria_label(field.label.clone())
        .key_context("SubspaceMarkdownField Input")
        .on_action(|_: &NextField, window, cx| window.focus_next(cx))
        .on_action(|_: &PreviousField, window, cx| window.focus_prev(cx))
        .h(field.height)
        .min_w_0()
        .overflow_hidden()
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(outline)
        .bg(tokens.surface)
        .when(!field.readonly, |frame| frame.child(toolbar(&rich, tokens)))
        .child(viewport(
            &rich,
            &focus,
            scroll,
            empty.then_some(placeholder),
            tokens,
        ))
        .into_any_element()
}

fn toolbar(rich: &Entity<BlockEditorState>, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .min_w_0()
        .px_1()
        .border_b_1()
        .border_color(tokens.line)
        .child(EditorToolbar::new(rich))
}

fn viewport(
    rich: &Entity<BlockEditorState>,
    focus: &gpui_kit::FocusHandle,
    scroll: Option<gpui_kit::ScrollHandle>,
    placeholder: Option<SharedString>,
    tokens: &Tokens,
) -> impl IntoElement {
    div()
        .id(("markdown-viewport", rich.entity_id()))
        .track_focus(focus)
        .tab_index(0)
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .when_some(scroll, |view, scroll| view.track_scroll(&scroll))
        .child(BlockEditor::new(rich).min_h_full().px_3().py_2())
        .when_some(placeholder, |view, text| {
            view.child(
                div()
                    .absolute()
                    .left(rems(INSET_X_REMS))
                    .top(rems(INSET_Y_REMS))
                    .text_color(tokens.muted)
                    .child(text),
            )
        })
}
