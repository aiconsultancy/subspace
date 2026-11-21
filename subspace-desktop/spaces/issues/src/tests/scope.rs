use std::time::Instant;

use subspace_api_client::hub::LiveStatus;

use super::{
    Access, Body, NoOrganisation, Screen, ScreenAction, body, context_line, scope_line,
    screen_actions, screen_text, status_line, with_hidden,
};
use crate::listing::tests::group;
use crate::listing::{Failure, Listing};
use crate::model::StateKind;

fn ada() -> Access {
    Access::Ready {
        display_name: "Ada Fixture".into(),
    }
}

#[test]
fn the_status_bar_carries_the_old_headings_context() {
    assert_eq!(
        context_line("Subspace API", &ada(), LiveStatus::Live),
        "Subspace API · Signed in as Ada Fixture · Live"
    );
    assert_eq!(
        context_line("Subspace API", &Access::Starting, LiveStatus::Live),
        "Subspace API",
        "no live word before signing in"
    );
    let now = Instant::now();
    let mut listing = Listing::default();
    listing.loaded(
        vec![
            group("s1", "In Progress", StateKind::Started, 4),
            group("s2", "Todo", StateKind::Unstarted, 40),
        ],
        now,
    );
    assert_eq!(
        status_line("Subspace API", &ada(), LiveStatus::Live, &listing, now),
        "Subspace API · Signed in as Ada Fixture · Live · 44 issues · updated just now"
    );
    listing.start_read();
    listing.failed(Failure::Offline);
    assert_eq!(
        status_line(
            "Subspace API",
            &ada(),
            LiveStatus::Reconnecting,
            &listing,
            now
        ),
        "Subspace API · Signed in as Ada Fixture · Offline"
    );
    let mut loading = Listing::default();
    loading.start_read();
    assert_eq!(
        status_line(
            "Subspace API",
            &ada(),
            LiveStatus::Connecting,
            &loading,
            now
        ),
        "Subspace API · Signed in as Ada Fixture · Connecting… · Loading issues…"
    );
    assert_eq!(
        status_line(
            "Subspace API",
            &Access::Failed(Failure::Edge),
            LiveStatus::Live,
            &Listing::default(),
            now
        ),
        "Subspace API · Sign-in required"
    );
}
#[test]
fn edge_and_network_failures_give_the_required_recovery_copy() {
    let edge = Screen::Failed(Failure::Edge);
    assert_eq!(
        screen_text(&edge, "Subspace API").message,
        "Sign in under Settings › Account, then try again."
    );
    assert_eq!(screen_actions(&edge), [ScreenAction::Retry]);
    assert_eq!(
        screen_text(&Screen::Failed(Failure::Network), "Subspace API").message,
        "Couldn't reach Subspace API. Try again."
    );
}
#[test]
fn connection_states_control_the_first_load_and_scoped_heading() {
    let listing = Listing::default();
    assert_eq!(
        body(&Access::Starting, &listing),
        Body::Loading { controls: false }
    );
    let ready = Access::Ready {
        display_name: "Ada Fixture".into(),
    };
    assert_eq!(body(&ready, &listing), Body::Loading { controls: true });
    assert_eq!(
        scope_line("Subspace API", &ready),
        "Subspace API · Signed in as Ada Fixture"
    );
    assert!(!body(&Access::Failed(Failure::Edge), &listing).controls());
}
#[test]
fn an_organisation_to_choose_and_none_to_use_read_differently() {
    use subspace_api_client::nett::Status;
    let choose = NoOrganisation::from_status(&Status::Choose);
    assert_eq!(choose, NoOrganisation::Choose);
    assert!(choose.message().contains("Organisation menu"));
    assert!(choose.actions().is_empty(), "the menu is the way forward");
    let none = NoOrganisation::from_status(&Status::None);
    assert_eq!(none, NoOrganisation::NoneEligible);
    assert_eq!(none.title(), "No organisation available");
    assert!(
        !none.message().contains("menu"),
        "no menu appears without a choice"
    );
    assert_eq!(none.actions(), [ScreenAction::Retry]);
    // Before the organisations are read, the screen asks for a choice.
    let screen = Screen::Failed(Failure::Organisation);
    assert_eq!(screen_text(&screen, "NETT").title, "Choose an organisation");
    assert!(!screen_text(&screen, "NETT").error);
    assert_eq!(
        scope_line("NETT", &Access::Failed(Failure::Organisation)),
        "NETT · Choose an organisation"
    );
}

#[test]
fn the_status_bar_says_how_many_sub_issues_the_list_hides() {
    assert_eq!(
        with_hidden(
            "Subspace API · 3 issues · updated just now".into(),
            Some(12)
        ),
        "Subspace API · 3 issues · updated just now · 12 sub-issues hidden"
    );
    assert_eq!(
        with_hidden("Subspace API · 3 issues".into(), None),
        "Subspace API · 3 issues"
    );
}
