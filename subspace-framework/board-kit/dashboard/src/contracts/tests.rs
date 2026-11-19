use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{App, AppContext as _, Entity};
use subspace_contracts::{Contract, Event, MessageKey, Request};

use super::broker::Receivers;
use super::{BoardContracts, Declaration, Role, subscribe, validate};

mod events;
mod requests;

struct Fixture;

impl Contract for Fixture {
    const NAME: &'static str = "fixture";
    const VERSION: u32 = 1;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Ping(u32);

impl Event for Ping {
    type Contract = Fixture;
    const NAME: &'static str = "ping";
}

struct Double(u32);

impl Request for Double {
    type Contract = Fixture;
    const NAME: &'static str = "double";
    type Response = u32;
}

struct Wrong;

impl Contract for Wrong {
    const NAME: &'static str = "Wrong";
    const VERSION: u32 = 1;
}

struct Bad;

impl Event for Bad {
    type Contract = Wrong;
    const NAME: &'static str = "bad";
}

const PUBLISHER: BoardContracts = BoardContracts::new(&[Declaration::publishes::<Ping>()]);
const SUBSCRIBER: BoardContracts = BoardContracts::new(&[Declaration::subscribes::<Ping>()]);
const PING_PONG: BoardContracts = BoardContracts::new(&[
    Declaration::publishes::<Ping>(),
    Declaration::subscribes::<Ping>(),
]);
const ASKER: BoardContracts = BoardContracts::new(&[Declaration::requests::<Double>()]);
const ANSWERER: BoardContracts = BoardContracts::new(&[
    Declaration::responds::<Double>(),
    Declaration::requests::<Double>(),
]);

/// Who received what, across receivers, in delivery order.
type Log = Rc<RefCell<Vec<(&'static str, u32)>>>;

struct Receiver;

fn receiver(board: &'static str, log: &Log, cx: &mut App) -> Entity<Receiver> {
    let entity = cx.new(|_| Receiver);
    let log = log.clone();
    let subscribed = subscribe(
        board,
        &entity,
        move |_: &mut Receiver, ping: &Ping, _| log.borrow_mut().push((board, ping.0)),
        cx,
    );
    assert_eq!(subscribed, Ok(()));
    entity
}

fn receivers(cx: &mut App) -> usize {
    cx.default_global::<Receivers<Ping>>().0.len()
}

#[test]
fn validation_rejects_bad_names_repeats_and_a_second_responder() {
    const BAD: BoardContracts = BoardContracts::new(&[Declaration::publishes::<Bad>()]);
    const TWICE: BoardContracts = BoardContracts::new(&[
        Declaration::publishes::<Ping>(),
        Declaration::publishes::<Ping>(),
    ]);
    assert!(validate(&[("alpha", PING_PONG), ("beta", ANSWERER), ("gamma", ASKER)]).is_ok());
    assert!(validate(&[("alpha", BAD)]).is_err_and(|error| error.contains("Wrong@1/bad")));
    assert!(validate(&[("alpha", TWICE)]).is_err_and(|error| error.contains("twice")));
    let second = validate(&[("alpha", ANSWERER), ("beta", ANSWERER)]);
    assert!(second.is_err_and(|error| error.contains("more than one board responds")));
    assert!(ANSWERER.declares(Role::Responds, MessageKey::request::<Double>()));
    assert!(!ANSWERER.declares(Role::Responds, MessageKey::event::<Ping>()));
}

#[cfg(feature = "test-support")]
#[gpui_kit::test]
fn the_recorder_captures_what_a_board_publishes(cx: &mut gpui_kit::TestAppContext) {
    use super::publish;
    use crate::test_support::contracts::Recorder;
    let recorder = cx.update(|cx| Recorder::<Ping>::install(&[("alpha", PUBLISHER)], cx));
    cx.update(|cx| {
        assert_eq!(publish("alpha", Ping(7), cx), Ok(()));
        assert!(recorder.events(cx).is_empty());
    });
    cx.update(|cx| assert_eq!(recorder.events(cx), [Ping(7)]));
}
