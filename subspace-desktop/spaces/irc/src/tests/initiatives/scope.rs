use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::initiatives::Catalog;
use subspace_client_irc::Command;

use super::super::{mount, settings};
use super::{catalogue, settle};
use crate::fixtures::network_id;
use crate::hub::Hub;

const OBSERVATORY: &str = "99469809-1f67-4f56-9d66-47a052e0e92b";

#[gpui_kit::test]
fn another_organisation_takes_the_initiative_autojoin_with_it(cx: &mut TestAppContext) {
    let (hub, catalog, cx) = mount_with_selected_initiative(cx);
    let joined = Command::SetAutojoin {
        network: network_id(),
        channels: Some(vec!["#data".into(), "#forecast".into()]),
    };
    hub.read_with(cx, |hub, _| assert!(hub.sent().contains(&joined)));
    // The same person in another organisation: the selection and its
    // autojoin go, before anything is read for the new organisation.
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(2), cx));
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::SetAutojoin {
                network: network_id(),
                channels: None,
            })
        );
        assert!(!hub.initiatives.selected_for_runtime());
    });
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.selected().is_none() && !catalog.has_memberships());
    });
}

#[gpui_kit::test]
fn a_read_in_flight_for_the_old_organisation_never_lands_in_the_new_one(cx: &mut TestAppContext) {
    let (hub, catalog, cx) = mount_with_selected_initiative(cx);
    // Organisation A's read leaves; it is still in flight when the switch
    // to organisation B retires A's catalogue, and its answer is dropped.
    catalog.update(cx, |catalog, cx| {
        catalog.refresh_for_test(
            Some(1),
            || (Some("fixture-user".into()), Ok(catalogue())),
            cx,
        );
        catalog.change_scope_for_test(Some(2), cx);
    });
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::SetAutojoin {
                network: network_id(),
                channels: None,
            })
        );
    });
    catalog.read_with(cx, |catalog, _| {
        assert!(!catalog.has_memberships() && catalog.selected().is_none());
    });
    hub.read_with(cx, |hub, _| {
        assert!(!hub.initiatives.selected_for_runtime());
    });
}

fn mount_with_selected_initiative(
    cx: &mut TestAppContext,
) -> (Entity<Hub>, Entity<Catalog>, &mut VisualTestContext) {
    let (_board, hub, cx) = mount(cx, settings());
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.change_scope_for_test(Some(1), cx);
        catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(catalogue()), cx);
        catalog.pick(OBSERVATORY, cx);
    });
    settle(cx);
    (hub, catalog, cx)
}
