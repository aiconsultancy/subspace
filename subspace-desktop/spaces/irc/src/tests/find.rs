use super::{inject, mount, settings};
use crate::{
    conversation,
    fixtures::{channel, line, network_id},
    view,
};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::{ConnectionStatus, Event, LineKind};
use subspace_framework::dashboard::Dashboard;
#[gpui_kit::test]
fn find_supplies_loaded_offscreen_messages_and_reveals_their_native_row(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#find-fixture");
    let mut events = vec![
        Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected {
                nick: "subspace".into(),
            },
        },
        Event::Joined {
            buffer: room.clone(),
        },
    ];
    events.extend((0..100).map(|index| {
        Event::Line(line(
            &room,
            LineKind::Privmsg,
            "fixture-bot",
            &format!("loaded message {index}"),
            index + 1,
        ))
    }));
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let conversation = board.read_with(cx, |board, _| {
        board.dock.open.get(&room).unwrap().entity.clone()
    });
    let docs = cx.update(|_, cx| conversation::find::documents(conversation.read(cx), cx));
    assert_eq!(docs.len(), 100);
    assert!(docs[0].1.markdown.contains("loaded message 0"));
    assert!(docs[99].1.markdown.contains("loaded message 99"));
    let first_row = hub.read_with(cx, |hub, _| {
        format!(
            "irc-line:{}:row",
            hub.state.buffer(&room).unwrap().entries[0].seq
        )
    });
    cx.update(|window, _| {
        assert!(
            window
                .try_find(first_row.clone())
                .is_none_or(|row| !row.visible())
        )
    });
    let key = docs[0].1.key();
    conversation.update(cx, |view, cx| conversation::find::reveal(view, &key, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find(first_row).visible());
        assert_eq!(Dashboard::find_documents(board.read(cx), cx).1.len(), 100);
        assert!(Dashboard::find_virtual_content(board.read(cx), cx));
    });
}

#[gpui_kit::test]
fn an_open_attachment_uses_its_own_find_scope_instead_of_the_channel(cx: &mut TestAppContext) {
    let (board, _, cx) = mount(cx, settings());
    let room = channel("#attachment-find");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    let conversation = board.read_with(cx, |board, _| {
        board.dock.open.get(&room).unwrap().entity.clone()
    });
    let original = board.read_with(cx, Dashboard::find_documents).0;
    let open = crate::uploads::viewer::claim(&conversation.downgrade());
    cx.update(|window, cx| open("UPL-THz3d1BhX6ieQk91Zzwrww", window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let board = board.read(cx);
        assert!(!Dashboard::find_virtual_content(board, cx));
        let (scope, documents) = Dashboard::find_documents(board, cx);
        assert_ne!(scope, original);
        assert!(documents.is_empty());
        window.click("chat-upload-viewer-close", cx);
    });
    cx.run_until_parked();
    board.read_with(cx, |board, cx| {
        assert!(Dashboard::find_virtual_content(board, cx));
        assert_eq!(Dashboard::find_documents(board, cx).0, original);
    });
}

#[gpui_kit::test]
fn open_find_follows_attachment_open_and_close_without_reopening_find(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, super::channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    let conversation = board.read_with(cx, |board, _| {
        board.dock.open.get(&room).unwrap().entity.clone()
    });
    attach_find(&board, cx);
    cx.update(|window, cx| {
        window.press(subspace_framework::dashboard::secondary!("f"), cx);
        window.input("rocket", cx);
    });
    cx.run_until_parked();
    assert_count("Match 1 of 1", cx);
    open_attachment(&conversation, cx);
    assert!(conversation.read_with(cx, |view, _| view.upload_viewer.is_some()));
    assert_count("Match 0 of 0", cx);
    close_attachment(cx);
    assert_count("Match 1 of 1", cx);
}

pub(super) fn attach_find(
    board: &gpui_kit::Entity<crate::Irc>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.update(|window, cx| {
        let handle = subspace_framework::dashboard::DashboardHandle::from_entity(board.clone());
        let find = handle.find_view(window, cx);
        window.replace_root(cx, |window, cx| gpui_kit::base::Root::new(find, window, cx));
        window.render_frame(cx);
    });
    cx.run_until_parked();
}
pub(super) fn assert_count(expected: &str, cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-match-count").label(), Some(expected));
    });
}
pub(super) fn open_attachment(
    conversation: &gpui_kit::Entity<crate::conversation::Conversation>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.update(|window, cx| {
        crate::uploads::viewer::claim(&conversation.downgrade())(
            "UPL-THz3d1BhX6ieQk91Zzwrww",
            window,
            cx,
        )
    });
    cx.run_until_parked();
}
pub(super) fn close_attachment(cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| window.click("chat-upload-viewer-close", cx));
    cx.run_until_parked();
}
mod panes;

#[gpui_kit::test]
fn unrelated_and_hidden_hub_events_do_not_refresh_the_open_find_provider(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, super::channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |b, cx| view::reveal(b, &room, cx));
    cx.run_until_parked();
    let conversation = board.read_with(cx, |b, _| b.dock.open.get(&room).unwrap().entity.clone());
    attach_find(&board, cx);
    cx.update(|window, cx| window.press(subspace_framework::dashboard::secondary!("f"), cx));
    cx.run_until_parked();
    let before = provider_calls(&conversation, cx);
    inject(
        &hub,
        vec![Event::Line(line(
            &channel("#other"),
            LineKind::Privmsg,
            "fixture",
            "unrelated",
            500,
        ))],
        cx,
    );
    cx.run_until_parked();
    assert_eq!(provider_calls(&conversation, cx), before);
    inject(
        &hub,
        vec![Event::Line(line(
            &room,
            LineKind::Privmsg,
            "fixture",
            "new focused message",
            501,
        ))],
        cx,
    );
    cx.run_until_parked();
    let after = provider_calls(&conversation, cx);
    assert!(after > before);
    cx.update(|window, cx| board.update(cx, |b, cx| b.deactivate(window, cx)));
    inject(
        &hub,
        vec![Event::Line(line(
            &room,
            LineKind::Privmsg,
            "fixture",
            "hidden message",
            502,
        ))],
        cx,
    );
    cx.run_until_parked();
    assert_eq!(provider_calls(&conversation, cx), after);
    cx.update(|window, cx| board.update(cx, |b, cx| b.activate(window, cx)));
    cx.run_until_parked();
    assert!(provider_calls(&conversation, cx) > after);
}

fn provider_calls(
    conversation: &gpui_kit::Entity<crate::conversation::Conversation>,
    cx: &mut gpui_kit::VisualTestContext,
) -> usize {
    conversation.read_with(cx, |view, _| view.find_cache.borrow().calls)
}

#[gpui_kit::test]
fn replacing_the_hub_clears_an_open_find_even_if_the_old_hub_is_retained(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, super::channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |b, cx| view::reveal(b, &room, cx));
    cx.run_until_parked();
    attach_find(&board, cx);
    cx.update(|window, cx| {
        window.press(subspace_framework::dashboard::secondary!("f"), cx);
        window.input("rocket", cx);
    });
    cx.run_until_parked();
    assert_count("Match 1 of 1", cx);
    cx.update(|_, cx| {
        crate::hub::install_for_test(&settings(), cx);
    });
    board.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    assert_count("Match 0 of 0", cx);
    assert!(hub.read_with(cx, |hub, _| hub.state.buffer(&room).is_some()));
}

#[gpui_kit::test]
fn stopping_the_hub_from_render_clears_the_open_find(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, super::channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |b, cx| view::reveal(b, &room, cx));
    cx.run_until_parked();
    attach_find(&board, cx);
    cx.update(|window, cx| {
        window.press(subspace_framework::dashboard::secondary!("f"), cx);
        window.input("rocket", cx);
    });
    cx.run_until_parked();
    assert_count("Match 1 of 1", cx);
    cx.update(|_, cx| {
        cx.remove_global::<crate::hub::Process>();
    });
    board.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    assert_count("Match 0 of 0", cx);
    assert!(hub.read_with(cx, |hub, _| hub.state.buffer(&room).is_some()));
}
