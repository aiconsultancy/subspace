//! Records the contract events a board publishes, for board tests.

use std::marker::PhantomData;

use gpui_kit::{App, AppContext as _, Entity};
use subspace_contracts::Event;

use crate::contracts::{self, BoardContracts, Declaration};

/// The board ID of the recorder, routed after the boards under test.
pub const RECORDER: &str = "contract-recorder";

struct Subscribes<E>(PhantomData<E>);

impl<E: Event> Subscribes<E> {
    const DECLARATIONS: &'static [Declaration] = &[Declaration::subscribes::<E>()];
}

/// Captures every `E` the routed boards publish.
// qual:api — board tests use the recorder once boards publish contract events.
pub struct Recorder<E> {
    events: Entity<Vec<E>>,
}

impl<E: Event + Clone> Recorder<E> {
    /// Installs the broker with `boards`, then a recorder board that
    /// subscribes to `E`, as the host does for the running boards.
    pub fn install(boards: &[(&'static str, BoardContracts)], cx: &mut App) -> Self {
        let mut routed = boards.to_vec();
        routed.push((RECORDER, BoardContracts::new(Subscribes::<E>::DECLARATIONS)));
        contracts::install(routed, cx);
        let events = cx.new(|_| Vec::new());
        let subscribed = contracts::subscribe(
            RECORDER,
            &events,
            |events: &mut Vec<E>, event: &E, _| events.push(event.clone()),
            cx,
        );
        debug_assert!(subscribed.is_ok(), "the recorder declares its subscription");
        Self { events }
    }

    /// The events delivered so far, in delivery order.
    pub fn events(&self, cx: &App) -> Vec<E> {
        self.events.read(cx).clone()
    }
}
