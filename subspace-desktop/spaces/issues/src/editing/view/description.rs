//! One issue description draft with optional Markdown syntax highlighting.

use crate::editing::{DESCRIPTION_MARKDOWN, DESCRIPTION_PLAIN, IssueEditor};
use crate::{Issues, view::reveal_focus};
use gpui_kit::component::{
    h_flex,
    input::{IndentInline, OutdentInline},
    v_flex,
};
use gpui_kit::{Context, Div, InteractiveElement as _, ParentElement as _, Styled as _, div, rems};
use subspace_framework::dashboard::{
    Tokens, fields,
    segmented::{self, Segment},
};

const HEIGHT_REMS: f32 = 24.;
const LANGUAGES: [&str; 2] = [DESCRIPTION_MARKDOWN, DESCRIPTION_PLAIN];

pub(super) fn render(
    editor: &IssueEditor,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .gap_2()
        .child(reveal_focus::scope(
            "description-mode",
            &editor.scroll,
            h_flex()
                .gap_2()
                .flex_wrap()
                .justify_between()
                .child("Description")
                .child(mode(editor, tokens, cx)),
        ))
        .child(reveal_focus::scope(
            "description-input",
            &editor.scroll,
            div()
                .id("issue-edit-description")
                // In a form, Tab leaves the description for the next field.
                .capture_action(|_: &IndentInline, window, cx| {
                    cx.stop_propagation();
                    window.focus_next(cx);
                })
                .capture_action(|_: &OutdentInline, window, cx| {
                    cx.stop_propagation();
                    window.focus_prev(cx);
                })
                .child(
                    fields::code_editor(&editor.description, "Issue description")
                        .h(rems(HEIGHT_REMS))
                        .readonly(locked),
                ),
        ))
}

fn mode(editor: &IssueEditor, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let language = editor.description.read(cx).language_name();
    let board = cx.weak_entity();
    segmented::control(
        "issue-description-mode",
        [Segment::new("Markdown"), Segment::new("Plain text")],
        selected_mode(language.as_ref()),
        tokens,
        move |index, _, _, cx| {
            _ = board.update(cx, |board, cx| select_mode(board, LANGUAGES[index], cx));
        },
    )
}

fn selected_mode(language: &str) -> Option<usize> {
    LANGUAGES.iter().position(|choice| *choice == language)
}

fn select_mode(board: &Issues, language: &'static str, cx: &mut Context<Issues>) {
    let Some(editor) = &board.editor else {
        return;
    };
    editor
        .description
        .update(cx, |state, cx| state.set_highlighter(language, cx));
    cx.notify();
}
