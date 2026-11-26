use std::path::Path;

use super::super::completion::{composer, text};
use crate::uploads::outgoing::ItemState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ExternalPaths, FileDropEvent, InputEvent as _, TestAppContext, point, px};

fn file(folder: &Path, name: &str) -> std::path::PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, "Example attachment").unwrap();
    path
}

#[gpui_kit::test]
fn the_paperclip_opens_a_file_picker_with_an_empty_draft(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    let folder = tempfile::tempdir().unwrap();
    let files = vec![
        file(folder.path(), "agenda.txt"),
        file(folder.path(), "notes.txt"),
    ];
    cx.update(|window, cx| window.click("irc-attach-file", cx));
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert!(conversation.read_with(cx, |view, _| view.tray.is_empty()));

    cx.update(|window, cx| window.click("irc-attach-file", cx));
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.files && options.multiple && !options.directories);
        Some(files)
    });
    cx.run_until_parked();
    assert!(text(&input, cx).is_empty());
    conversation.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), 2);
        for (item, name) in view.tray.items.iter().zip(["agenda.txt", "notes.txt"]) {
            let ItemState::Ready(pending) = &item.state else {
                panic!("selected files should be ready");
            };
            assert_eq!(pending.draft.name(), name);
            assert!(pending.uploaded.is_none());
        }
    });
}

#[gpui_kit::test]
fn a_file_dropped_over_the_focused_text_box_keeps_the_draft(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    let folder = tempfile::tempdir().unwrap();
    let path = file(folder.path(), "fixture.txt");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Keep this draft", window, cx)
        });
        window.render_frame(cx);
        let position = window.find("chat-input").bounds().center();
        window.dispatch_event(
            FileDropEvent::Entered {
                position: point(position.x, px(120.)),
                paths: ExternalPaths([path].into_iter().collect()),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(FileDropEvent::Pending { position }.to_platform_input(), cx);
        window.render_frame(cx);
        window.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
    });
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "Keep this draft");
    conversation.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), 1);
        let ItemState::Ready(pending) = &view.tray.items[0].state else {
            panic!("dropped file should be ready");
        };
        assert_eq!(pending.draft.name(), "fixture.txt");
        assert!(pending.uploaded.is_none());
    });
}
