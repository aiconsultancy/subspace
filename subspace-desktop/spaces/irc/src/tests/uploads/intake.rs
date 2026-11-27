use std::path::Path;

use super::super::completion::{composer, text};
use crate::uploads::outgoing::ItemState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ExternalPaths, FileDropEvent, InputEvent as _, TestAppContext};

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

const WEBP: &[u8] = b"RIFF\x24\0\0\0WEBPVP8 \x18\0\0\0\x30\x01\0\x9d\x01\x2a\x01\0\x01\0\x02\0\x34\x25\xa4\0\x03\x70\0\xfe\xfb\x94\0\0";

#[gpui_kit::test]
fn mixed_files_dropped_over_the_focused_text_box_keep_the_draft(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    let folder = tempfile::tempdir().unwrap();
    let image = folder.path().join("fixture.webp");
    std::fs::write(&image, WEBP).unwrap();
    let paths = [
        file(folder.path(), "fixture.txt"),
        file(folder.path(), "index.HTM"),
        image,
        file(folder.path(), "theme.css"),
    ];
    cx.simulate_input("Keep this draft");
    let position = cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.last_input_was_keyboard());
        let position = window.find("chat-input").bounds().center();
        window.dispatch_event(
            FileDropEvent::Entered {
                position,
                paths: ExternalPaths(paths.into_iter().collect()),
            }
            .to_platform_input(),
            cx,
        );
        position
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.dispatch_event(FileDropEvent::Pending { position }.to_platform_input(), cx);
        window.render_frame(cx);
        window.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
    });
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "Keep this draft");
    conversation.read_with(cx, |view, _| {
        assert_eq!(view.tray.items.len(), 4);
        let expected = [
            ("fixture.txt", "text/plain"),
            ("index.HTM", "text/plain"),
            ("fixture.webp", "image/webp"),
            ("theme.css", "text/plain"),
        ];
        for (item, (name, mime)) in view.tray.items.iter().zip(expected) {
            let ItemState::Ready(pending) = &item.state else {
                panic!("dropped file {name} should be ready");
            };
            assert_eq!((pending.draft.name(), pending.draft.mime()), (name, mime));
            assert!(pending.uploaded.is_none());
        }
    });
}
