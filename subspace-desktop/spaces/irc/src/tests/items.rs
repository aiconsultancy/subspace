//! `/` and the palette find channels and conversations by name, and open
//! them as recent items.

use gpui_kit::{Entity, Global, TestAppContext};
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent};

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{channel, message, query};
use crate::{Irc, rows};

#[derive(Default)]
struct Opened(Vec<String>);
impl Global for Opened {}

#[gpui_kit::test]
fn item_search_finds_buffers_by_name_and_network_and_opens_them(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let carol = query("carol");
    inject(
        &hub,
        vec![subspace_client_irc::Event::Line(message(
            &carol, "carol", "hi", 3,
        ))],
        cx,
    );
    cx.update(|_, cx| cx.set_global(Opened::default()));
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&board, |_: Entity<Irc>, event: &DashboardEvent, cx| {
            if let DashboardEvent::Opened(id) = event {
                cx.global_mut::<Opened>().0.push(id.to_string());
            }
        })
    });
    let search = cx.update(|_, cx| board.read(cx).search(cx)).unwrap();
    let titles = |query: &str| -> Vec<String> {
        search
            .items
            .search(query, 5)
            .into_iter()
            .map(|(_, row)| row.title.to_string())
            .collect()
    };
    assert_eq!(
        titles("subspace").first().map(String::as_str),
        Some("#subspace")
    );
    assert_eq!(titles("carol").first().map(String::as_str), Some("carol"));
    let key = rows::row_key(&channel("#subspace"));
    cx.update(|window, cx| {
        board.update(cx, |board, cx| board.open_search_result(&key, window, cx));
    });
    cx.run_until_parked();
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&channel("#subspace")));
    });
    let opened = cx.update(|_, cx| cx.global::<Opened>().0.clone());
    assert_eq!(opened, [key.to_string()]);
}
