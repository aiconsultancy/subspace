use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    InteractiveElement as _, IntoElement, ParentElement as _, Render, TestAppContext,
    VisualTestContext, div,
};

const ORIGINAL: &str = "## Heading\n\n* An _important_ detail\n";
const TABLE: &str = "| Key | Value |\n| --- | --- |\n| a | b |";
const MODE_ID: &str = "markdown-editor-test-mode";

actions!(markdown_field_test, [OpenPalette]);

struct Fixture {
    state: Entity<MarkdownEditor>,
    changes: usize,
    submissions: usize,
    palette_opens: usize,
    readonly: bool,
    _subscription: Subscription,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("MarkdownFieldTest")
            .on_action(cx.listener(|this, _: &OpenPalette, _, _| this.palette_opens += 1))
            .child(MarkdownField::new(&self.state, MODE_ID, "Description").readonly(self.readonly))
    }
}

fn editor(cx: &mut TestAppContext) -> (Entity<Fixture>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([KeyBinding::new(
            crate::secondary!("k"),
            OpenPalette,
            Some("MarkdownFieldTest"),
        )]);
    });
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| MarkdownEditor::new(ORIGINAL, "Write a description", window, cx));
        let subscription = cx.subscribe(&state, |this: &mut Fixture, _, event, _| match event {
            MarkdownEditorEvent::Changed => this.changes += 1,
            MarkdownEditorEvent::SubmitRequested => this.submissions += 1,
        });
        Fixture {
            state,
            changes: 0,
            submissions: 0,
            palette_opens: 0,
            readonly: false,
            _subscription: subscription,
        }
    });
    draw(cx);
    (fixture, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn switching_modes_keeps_original_bytes_and_does_not_emit_change(cx: &mut TestAppContext) {
    let (fixture, cx) = editor(cx);
    for index in [1_usize, 0] {
        cx.update(|window, cx| window.click((MODE_ID, index), cx));
        draw(cx);
        cx.update(|window, cx| {
            assert_eq!(window.find((MODE_ID, index)).checked(), Some(true));
            let fixture = fixture.read(cx);
            assert_eq!(fixture.state.read(cx).value(cx).as_ref(), ORIGINAL);
            assert_eq!(fixture.changes, 0);
        });
    }
}

#[gpui_kit::test]
fn formatted_edits_reach_source_and_undo_survives_a_mode_switch(cx: &mut TestAppContext) {
    let (fixture, cx) = editor(cx);
    cx.update(|window, cx| {
        let state = fixture.read(cx).state.clone();
        state.update(cx, |state, cx| state.focus(window, cx));
        window.input("New ", cx);
    });
    draw(cx);
    cx.update(|window, cx| window.click((MODE_ID, 1_usize), cx));
    draw(cx);
    let value = cx.update(|_, cx| {
        let fixture = fixture.read(cx);
        assert!(fixture.changes > 0);
        fixture.state.read(cx).value(cx)
    });
    assert!(value.starts_with("## New Heading"), "{value}");
    cx.update(|window, cx| window.click((MODE_ID, 0_usize), cx));
    draw(cx);
    cx.update(|window, cx| window.press(crate::secondary!("z"), cx));
    draw(cx);
    assert_eq!(
        cx.update(|_, cx| fixture.read(cx).state.read(cx).value(cx)),
        ORIGINAL
    );
}

#[gpui_kit::test]
fn readonly_formatted_field_blocks_edits(cx: &mut TestAppContext) {
    let (fixture, cx) = editor(cx);
    fixture.update(cx, |fixture, cx| {
        fixture.readonly = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|window, cx| {
        let state = fixture.read(cx).state.clone();
        state.update(cx, |state, cx| state.focus(window, cx));
        window.input("changed", cx);
    });
    draw(cx);
    cx.update(|_, cx| {
        let fixture = fixture.read(cx);
        assert_eq!(fixture.state.read(cx).value(cx), ORIGINAL);
        assert_eq!(fixture.changes, 0);
    });
}

#[gpui_kit::test]
fn formatted_table_preserves_the_save_and_palette_shortcuts(cx: &mut TestAppContext) {
    let (fixture, cx) = editor(cx);
    cx.update(|window, cx| {
        fixture.read(cx).state.clone().update(cx, |state, cx| {
            state.set_value(TABLE, window, cx);
            state.focus(window, cx);
        });
    });
    draw(cx);
    cx.update(|window, cx| {
        window.press(crate::secondary!("enter"), cx);
        window.press(crate::secondary!("k"), cx);
    });
    draw(cx);
    cx.update(|_, cx| {
        let fixture = fixture.read(cx);
        assert_eq!(fixture.submissions, 1);
        assert_eq!(fixture.palette_opens, 1);
        assert_eq!(fixture.changes, 0);
        assert_eq!(fixture.state.read(cx).value(cx), TABLE);
    });
}
