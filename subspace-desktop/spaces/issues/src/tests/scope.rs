use super::{
    Access, Body, NoOrganisation, Screen, ScreenAction, body, scope_line, screen_actions,
    screen_text,
};
use crate::listing::{Failure, Listing};
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
