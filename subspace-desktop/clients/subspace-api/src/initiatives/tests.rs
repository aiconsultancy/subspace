use gpui_kit::{AppContext as _, Entity, TestAppContext};
use serde_json::json;
use subspace_framework::dashboard::prefs;

use super::{CHECKING, Catalog, Error, NO_MEMBERSHIPS, UNAVAILABLE};
use crate::model::MemberInitiative;

mod state;

const DESIGN: &str = "dc94172e-aa0e-424a-8acd-8b7544b4ca0c";
const OPS: &str = "99469809-1f67-4f56-9d66-47a052e0e92b";
const DATA: &str = "5a3c4f7e-2b1d-4e8a-9c6f-0d7e8f9a1b2c";

fn initiatives(ids: &[&str]) -> Vec<MemberInitiative> {
    let items: Vec<_> = ids
        .iter()
        .map(|id| json!({"id": id, "name": format!("Initiative {id}"), "channels": "#fixture"}))
        .collect();
    serde_json::from_value(items.into()).unwrap()
}

/// A catalogue for one run of the app, which remembers choices in memory as
/// the state file would between runs.
fn run(cx: &mut TestAppContext) -> Entity<Catalog> {
    cx.update(|cx| {
        if !cx.has_global::<prefs::Prefs>() {
            prefs::init_file(None, cx);
        }
    });
    cx.new(|_| Catalog::default())
}

fn read(
    catalog: &Entity<Catalog>,
    actor: &str,
    result: Result<Vec<MemberInitiative>, Error>,
    cx: &mut TestAppContext,
) {
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some(actor.into()), result, cx);
    });
}

fn pick(catalog: &Entity<Catalog>, value: &str, cx: &mut TestAppContext) {
    catalog.update(cx, |catalog, cx| catalog.pick(value, cx));
}

fn selected(catalog: &Entity<Catalog>, cx: &mut TestAppContext) -> Option<String> {
    catalog.read_with(cx, |catalog, _| {
        catalog.selected().map(|initiative| initiative.id.clone())
    })
}

#[gpui_kit::test]
fn a_member_gets_their_first_initiative_once_the_memberships_are_known(cx: &mut TestAppContext) {
    let catalog = run(cx);
    assert_eq!(selected(&catalog, cx), None, "nothing is known yet");
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
    catalog.read_with(cx, |catalog, _| assert_eq!(catalog.hint().as_deref(), None));

    // A read that fails in transit keeps the selection; a new one keeps it
    // while the person still belongs to it.
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(None, Err(Error::Network), cx);
    });
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
    read(&catalog, "user-ada", Ok(initiatives(&[OPS, DESIGN])), cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
}

#[gpui_kit::test]
fn the_initiative_last_chosen_is_selected_again_in_a_later_run(cx: &mut TestAppContext) {
    let first = run(cx);
    read(&first, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    pick(&first, OPS, cx);
    assert_eq!(selected(&first, cx).as_deref(), Some(OPS));
    let stored = cx
        .update(|cx| prefs::get(cx, "initiatives.selected"))
        .unwrap();
    assert!(stored.contains(OPS));
    assert!(
        !stored.contains("user-ada") && !stored.contains(DESIGN),
        "only the chosen initiative's ID is kept: {stored}"
    );

    let later = run(cx);
    read(&later, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    assert_eq!(selected(&later, cx).as_deref(), Some(OPS));

    // Someone else on this computer starts from their own first initiative.
    let other = run(cx);
    read(&other, "user-bob", Ok(initiatives(&[DESIGN, OPS])), cx);
    assert_eq!(selected(&other, cx).as_deref(), Some(DESIGN));
}

#[gpui_kit::test]
fn a_remembered_initiative_the_person_left_falls_back_to_their_first(cx: &mut TestAppContext) {
    let first = run(cx);
    read(&first, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    pick(&first, OPS, cx);

    // Removed from Operations while the app runs: the next read moves the
    // selection to their first initiative.
    read(&first, "user-ada", Ok(initiatives(&[DATA, DESIGN])), cx);
    assert_eq!(selected(&first, cx).as_deref(), Some(DATA));

    // And in a later run, where Operations is remembered but gone.
    let later = run(cx);
    read(&later, "user-ada", Ok(initiatives(&[DESIGN, DATA])), cx);
    assert_eq!(selected(&later, cx).as_deref(), Some(DESIGN));
}

#[gpui_kit::test]
fn another_account_or_organisation_never_inherits_the_selection(cx: &mut TestAppContext) {
    let catalog = run(cx);
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(1), cx));
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    pick(&catalog, OPS, cx);

    // The same person in organisation 2: nothing of organisation 1 is
    // selected, before or after its memberships are read.
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(2), cx));
    assert_eq!(selected(&catalog, cx), None);
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));

    // Back in organisation 1, its own choice returns.
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(1), cx));
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(OPS));

    // Another account signs in: the old selection goes with the old
    // account, and the new one starts from its own first initiative.
    read(&catalog, "user-bob", Ok(initiatives(&[DESIGN, OPS])), cx);
    catalog.read_with(cx, |catalog, _| {
        assert_eq!(catalog.verified_for(), Some((Some(1), "user-bob")));
    });
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some("user-carol".into()), Err(Error::Network), cx);
    });
    assert_eq!(selected(&catalog, cx), None, "an unread account has none");
}

#[gpui_kit::test]
fn someone_in_no_initiative_is_told_they_are_not_a_member(cx: &mut TestAppContext) {
    let catalog = run(cx);
    read(&catalog, "user-ada", Ok(Vec::new()), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.selected().is_none() && catalog.no_memberships());
        assert_eq!(catalog.hint().as_deref(), Some(NO_MEMBERSHIPS));
        assert!(catalog.menu().options.is_empty());
    });
    pick(&catalog, DESIGN, cx);
    assert_eq!(selected(&catalog, cx), None);

    // A read that then fails in transit settles nothing: still none, and
    // the hint says why the read failed.
    read(&catalog, "user-ada", Err(Error::UnexpectedStatus(500)), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.no_memberships() && !catalog.memberships_known());
        assert_eq!(catalog.hint().as_deref(), Some(UNAVAILABLE));
    });

    // Added by an administrator: the next read selects it.
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN])), cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
    catalog.read_with(cx, |catalog, _| {
        assert!(!catalog.no_memberships());
        assert_eq!(catalog.hint().as_deref(), None);
    });

    // An API without the memberships endpoint says nothing about them.
    read(&catalog, "user-ada", Err(Error::NotFound), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.selected().is_none() && !catalog.memberships_known());
        assert!(!catalog.no_memberships());
        assert_eq!(catalog.hint().as_deref(), None);
    });
}

#[gpui_kit::test]
fn only_a_settled_read_says_the_person_belongs_to_no_initiative(cx: &mut TestAppContext) {
    let catalog = run(cx);
    // A read that fails before any settled is not taken for no memberships.
    read(&catalog, "user-ada", Err(Error::UnexpectedStatus(503)), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(!catalog.no_memberships());
        assert_eq!(catalog.hint().as_deref(), Some(UNAVAILABLE));
    });
    read(&catalog, "user-ada", Ok(Vec::new()), cx);
    catalog.read_with(cx, |catalog, _| assert!(catalog.no_memberships()));
    // A refusal answers for no one: it clears it.
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(None, Err(Error::Forbidden), cx);
    });
    catalog.read_with(cx, |catalog, _| assert!(!catalog.no_memberships()));
    // So does another organisation, until its own read settles.
    read(&catalog, "user-ada", Ok(Vec::new()), cx);
    catalog.update(cx, |catalog, cx| catalog.change_scope_for_test(Some(2), cx));
    catalog.read_with(cx, |catalog, _| assert!(!catalog.no_memberships()));
}

#[gpui_kit::test]
fn an_unreachable_api_is_named_as_every_space_names_it(cx: &mut TestAppContext) {
    let catalog = run(cx);
    read(&catalog, "user-ada", Err(Error::Network), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.unreachable() && !catalog.no_memberships());
        assert_eq!(catalog.hint(), Some(crate::unreachable_message()));
    });
    // Once a read settles, after none were found, the unreachable API is
    // still named, while Channels stays closed.
    read(&catalog, "user-ada", Ok(Vec::new()), cx);
    read(&catalog, "user-ada", Err(Error::Network), cx);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.unreachable() && catalog.no_memberships());
        assert_eq!(catalog.hint(), Some(crate::unreachable_message()));
    });
    // A refusal or another failure is not taken for an unreachable API.
    for (error, hint) in [
        (Error::Forbidden, None),
        (Error::UnexpectedStatus(500), Some(UNAVAILABLE)),
        (Error::InvalidResponse, Some(UNAVAILABLE)),
    ] {
        catalog.update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(None, Err(error), cx);
        });
        catalog.read_with(cx, |catalog, _| {
            assert!(!catalog.unreachable(), "{error:?}");
            assert_eq!(catalog.hint().as_deref(), hint, "{error:?}");
        });
    }
}

#[gpui_kit::test]
fn the_picker_offers_only_initiatives_while_the_person_has_any(cx: &mut TestAppContext) {
    let catalog = run(cx);
    read(&catalog, "user-ada", Ok(initiatives(&[DESIGN, OPS])), cx);
    catalog.read_with(cx, |catalog, _| {
        let menu = catalog.menu();
        let values: Vec<_> = menu.options.iter().map(|o| o.value.to_string()).collect();
        assert_eq!(
            values,
            [DESIGN, OPS],
            "no Local channels or other choice of none"
        );
        let checked: Vec<_> = menu.options.iter().map(|o| o.checked).collect();
        assert_eq!(checked, [true, false]);
    });
    // The value the old Local channels choice sent selects nothing.
    pick(&catalog, "local", cx);
    assert_eq!(selected(&catalog, cx).as_deref(), Some(DESIGN));
}

#[gpui_kit::test]
fn checking_shows_only_until_the_first_read_lands(cx: &mut TestAppContext) {
    let catalog = run(cx);
    catalog.update(cx, |catalog, cx| {
        catalog.refresh_for_test(None, || (Some("user-ada".into()), Ok(Vec::new())), cx);
        assert_eq!(catalog.hint().as_deref(), Some(CHECKING));
    });
    cx.run_until_parked();
    catalog.read_with(cx, |catalog, _| {
        assert_eq!(catalog.hint().as_deref(), Some(NO_MEMBERSHIPS))
    });
}
