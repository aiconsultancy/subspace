//! Contract declarations captured at registration, and routing that follows
//! the running boards' background work.

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, Render, TestAppContext, Window, div,
};
use subspace_contracts::issues::{Changed, Resolution, ResolutionState, Resolve};
use subspace_contracts::{MessageKey, Request};
use subspace_core::config::Config;
use subspace_dashboard::contracts::{
    BoardContracts, ContractError, Declaration, install, navigation_target, publish, request,
    respond, subscribe,
};
use subspace_dashboard::{Dashboard, DashboardEvent};
use subspace_product::{BoardDescriptor, validate_product};

use crate::registry::{BoardFactories, BoardRegistration, registration_sets, sync_backgrounds};

const RESPONDER: BoardContracts = BoardContracts::new(&[
    Declaration::responds::<Resolve>(),
    Declaration::publishes::<Changed>(),
]);
const LISTENER: BoardContracts = BoardContracts::new(&[
    Declaration::subscribes::<Changed>(),
    Declaration::requests::<Resolve>(),
]);

/// A board that answers `resolve`; registration must never create it.
struct Tracker {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for Tracker {}

impl Focusable for Tracker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Tracker {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Dashboard for Tracker {
    const ID: &'static str = "tracker";
    const TITLE: &'static str = "Tracker";

    fn contracts() -> BoardContracts {
        RESPONDER
    }

    fn start_background(cx: &mut App) {
        start_tracker(cx);
    }

    fn stop_background(cx: &mut App) {
        stop_tracker(cx);
    }

    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        panic!("contract routing must not create the tracker's view")
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn registration(id: &'static str, contracts: BoardContracts) -> BoardRegistration {
    let mut board = BoardRegistration::new::<Tracker>();
    board.id = id;
    board.contracts = contracts;
    board.start_background = |_| {};
    board.stop_background = |_| {};
    board
}

fn product(ids: &[&str]) -> subspace_product::ValidatedProduct {
    let mut descriptor = subspace_product::fixture::descriptor();
    descriptor.boards = ids
        .iter()
        .map(|id| BoardDescriptor {
            id: (*id).into(),
            aliases: Vec::new(),
            default_enabled: true,
        })
        .collect();
    validate_product(&descriptor).unwrap()
}

#[test]
fn declarations_are_listed_without_creating_the_board() {
    let native = BoardRegistration::new::<Tracker>();
    assert_eq!(native.contracts, RESPONDER);
    assert_eq!(native.contracts.declarations().len(), 2);
    let configured = BoardRegistration::configured::<Tracker>(
        "configured-contracts",
        "Configured",
        "Fixture",
        |_, _| panic!("listing must not create a configured board"),
    );
    // A configured board declares its type's roles under its own ID.
    assert_eq!(configured.contracts, RESPONDER);
}

#[test]
fn a_product_with_two_responders_to_one_request_is_rejected() {
    let boards = [
        BoardRegistration::new::<Tracker>(),
        registration("second", RESPONDER),
    ];
    let sets = registration_sets(
        &product(&["tracker", "second"]),
        &boards,
        &Config::default(),
    );
    assert!(sets.is_err_and(|error| error.contains("issues@1/resolve")));
    let single = registration_sets(&product(&["tracker"]), &boards[..1], &Config::default());
    assert!(single.is_ok());
}

/// The responder's process-owned state, created in `start_background`.
struct Hub;

/// Holds the hub alive, as a board's process-owned global does.
struct TrackerHub {
    _hub: Entity<Hub>,
}
impl Global for TrackerHub {}

/// How many `Changed` events the listener's process-owned state received.
struct Listener(usize);
struct ListenerHub(Entity<Listener>);
impl Global for ListenerHub {}

fn start_tracker(cx: &mut App) {
    let hub = cx.new(|_| Hub);
    let answer = |_: &mut Hub, ask: &Resolve, _: &mut Context<Hub>| {
        ask.identifiers()
            .iter()
            .map(|identifier| Resolution::new(identifier, ResolutionState::Pending))
            .collect()
    };
    assert_eq!(respond(Tracker::ID, &hub, answer, cx), Ok(()));
    assert_eq!(publish(Tracker::ID, Changed, cx), Ok(()));
    cx.set_global(TrackerHub { _hub: hub });
}

fn stop_tracker(cx: &mut App) {
    assert_eq!(publish(Tracker::ID, Changed, cx), Ok(()));
    cx.remove_global::<TrackerHub>();
}

fn start_listener(cx: &mut App) {
    let listener = cx.new(|_| Listener(0));
    let heard = |listener: &mut Listener, _: &Changed, _: &mut Context<Listener>| {
        listener.0 += 1;
    };
    assert_eq!(subscribe("listener", &listener, heard, cx), Ok(()));
    cx.set_global(ListenerHub(listener));
}

fn heard(cx: &App) -> usize {
    cx.global::<ListenerHub>().0.read(cx).0
}

fn resolve(cx: &mut App) -> Result<<Resolve as Request>::Response, ContractError> {
    request("listener", &Resolve::new("subspace-api", ["TST-1"]), cx)
}

#[gpui_kit::test]
fn background_work_receives_and_answers_without_a_view(cx: &mut TestAppContext) {
    let tracker = BoardRegistration::new::<Tracker>();
    let mut listener = registration("listener", LISTENER);
    listener.start_background = start_listener;
    cx.update(|cx| {
        cx.set_global(BoardFactories(vec![listener.clone(), tracker]));
        sync_backgrounds(cx);
    });
    cx.update(|cx| {
        assert_eq!(heard(cx), 1, "the tracker announced that it started");
        let resolutions = resolve(cx).map(|resolutions| resolutions.len());
        assert_eq!(resolutions, Ok(1));
        assert_eq!(
            navigation_target("listener", MessageKey::request::<Resolve>(), cx),
            Some(Tracker::ID)
        );
        cx.set_global(BoardFactories(vec![listener]));
        sync_backgrounds(cx);
    });
    cx.update(|cx| {
        assert_eq!(heard(cx), 2, "the tracker announced that it stopped");
        let unavailable = ContractError::Unavailable {
            key: MessageKey::request::<Resolve>(),
        };
        assert!(resolve(cx).is_err_and(|error| error == unavailable));
        assert_eq!(
            navigation_target("listener", MessageKey::request::<Resolve>(), cx),
            None
        );
        install(Vec::new(), cx);
    });
}
