//! A member always has an initiative selected once their memberships are
//! read: their first, or the one they last chose. Its channels join without
//! taking the conversation in front.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::{Command, Event};

use super::super::{inject, mount, settings};
use super::{member_of_the_catalogue, settle};
use crate::fixtures::{channel, network_id};

const OBSERVATORY: &str = "99469809-1f67-4f56-9d66-47a052e0e92b";

#[gpui_kit::test]
fn a_member_who_never_opens_the_picker_joins_their_first_initiative(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    member_of_the_catalogue(&hub, cx);
    settle(cx);
    let first = Command::SetAutojoin {
        network: network_id(),
        channels: Some(vec!["#design".into(), "#releases".into()]),
    };
    hub.read_with(cx, |hub, _| assert!(hub.sent().contains(&first)));
    inject(
        &hub,
        vec![Event::Joined {
            buffer: channel("#design"),
        }],
        cx,
    );
    board.read_with(cx, |board, _| {
        assert!(
            !board.dock.open.contains_key(&channel("#design")),
            "joined without taking the conversation in front"
        );
    });

    // The picker names it, and offers the person's initiatives only.
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("irc-initiative-button").label(),
            Some("Initiative: Design Studio")
        );
    });
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.read_with(cx, |catalog, _| {
        let labels: Vec<_> = catalog
            .menu()
            .options
            .iter()
            .map(|option| option.label.to_string())
            .collect();
        assert_eq!(labels, ["Design Studio", "Observatory"]);
    });

    // Switching to another initiative shows its first channel.
    catalog.update(cx, |catalog, cx| catalog.pick(OBSERVATORY, cx));
    settle(cx);
    inject(
        &hub,
        vec![Event::Joined {
            buffer: channel("#data"),
        }],
        cx,
    );
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&channel("#data")));
    });
}

#[gpui_kit::test]
fn a_fallback_joins_without_taking_the_conversation_in_front(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    member_of_the_catalogue(&hub, cx);
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| catalog.pick(OBSERVATORY, cx));
    settle(cx);
    // Removed from Observatory: Design Studio is selected for them, not by
    // them, so its channel joins without being brought to the front.
    let design: Vec<subspace_api_client::model::MemberInitiative> = serde_json::from_value(
        serde_json::json!([{"id": "dc94172e-aa0e-424a-8acd-8b7544b4ca0c",
                            "name": "Design Studio", "channels": "#design, #releases"}]),
    )
    .unwrap();
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(design), cx)
    });
    settle(cx);
    inject(
        &hub,
        vec![Event::Joined {
            buffer: channel("#design"),
        }],
        cx,
    );
    board.read_with(cx, |board, _| {
        assert!(!board.dock.open.contains_key(&channel("#design")));
    });
}

#[gpui_kit::test]
fn another_organisation_does_not_join_the_configured_channels_in_between(cx: &mut TestAppContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(1), cx));
    member_of_the_catalogue(&hub, cx);
    settle(cx);
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    // The same person switches organisation; its memberships are read.
    let other: Vec<subspace_api_client::model::MemberInitiative> = serde_json::from_value(
        serde_json::json!([{"id": "0f6c1b7e-3a2d-4c5e-8f9a-1b2c3d4e5f60",
                            "name": "Harbour", "channels": "#harbour"}]),
    )
    .unwrap();
    catalog.update(cx, |catalog, cx| {
        catalog.refresh_for_test(
            Some(2),
            move || (Some("fixture-user".into()), Ok(other)),
            cx,
        );
    });
    settle(cx);
    hub.read_with(cx, |hub, _| {
        let after: Vec<_> = hub.sent()[before..]
            .iter()
            .filter(|command| matches!(command, Command::SetAutojoin { .. }))
            .collect();
        assert_eq!(
            after,
            [&Command::SetAutojoin {
                network: network_id(),
                channels: Some(vec!["#harbour".into()]),
            }],
            "straight from one initiative to the next"
        );
    });
}
