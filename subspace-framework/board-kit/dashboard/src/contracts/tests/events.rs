use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{AppContext as _, TestAppContext};
use subspace_contracts::MessageKey;

use super::super::broker::MAX_DEPTH;
use super::super::{ContractError, Role, install, publish, subscribe};
use super::{Log, PING_PONG, PUBLISHER, Ping, Receiver, SUBSCRIBER, receiver, receivers};

#[gpui_kit::test]
fn events_wait_for_the_publisher_and_reach_subscribers_in_board_order(cx: &mut TestAppContext) {
    let log = Log::default();
    let _receivers = cx.update(|cx| {
        install(
            vec![
                ("alpha", PUBLISHER),
                ("gamma", SUBSCRIBER),
                ("beta", SUBSCRIBER),
            ],
            cx,
        );
        [receiver("beta", &log, cx), receiver("gamma", &log, cx)]
    });
    cx.update(|cx| {
        for number in 1..=2 {
            assert_eq!(publish("alpha", Ping(number), cx), Ok(()));
        }
        assert!(
            log.borrow().is_empty(),
            "delivery waits for the update to end"
        );
    });
    assert_eq!(
        *log.borrow(),
        [("gamma", 1), ("beta", 1), ("gamma", 2), ("beta", 2)]
    );
}

#[gpui_kit::test]
fn a_board_does_not_receive_its_own_events(cx: &mut TestAppContext) {
    let log = Log::default();
    let _receiver = cx.update(|cx| {
        install(vec![("alpha", PING_PONG)], cx);
        receiver("alpha", &log, cx)
    });
    cx.update(|cx| assert_eq!(publish("alpha", Ping(1), cx), Ok(())));
    assert!(log.borrow().is_empty());
}

#[gpui_kit::test]
fn a_dropped_receiver_is_skipped_and_forgotten(cx: &mut TestAppContext) {
    let log = Log::default();
    let weak = cx.update(|cx| {
        install(vec![("alpha", PUBLISHER), ("beta", SUBSCRIBER)], cx);
        receiver("beta", &log, cx).downgrade()
    });
    assert!(
        weak.upgrade().is_none(),
        "the broker holds no strong handle"
    );
    cx.update(|cx| {
        assert_eq!(receivers(cx), 1);
        assert_eq!(publish("alpha", Ping(1), cx), Ok(()));
    });
    assert!(log.borrow().is_empty());
    cx.update(|cx| assert_eq!(receivers(cx), 0));
}

#[gpui_kit::test]
fn a_board_that_is_not_running_receives_nothing_until_it_runs_again(cx: &mut TestAppContext) {
    let log = Log::default();
    let _receiver = cx.update(|cx| {
        install(vec![("alpha", PUBLISHER), ("beta", SUBSCRIBER)], cx);
        let entity = receiver("beta", &log, cx);
        install(vec![("alpha", PUBLISHER)], cx);
        assert_eq!(publish("alpha", Ping(1), cx), Ok(()));
        entity
    });
    assert!(log.borrow().is_empty());
    cx.update(|cx| {
        install(vec![("alpha", PUBLISHER), ("beta", SUBSCRIBER)], cx);
        assert_eq!(publish("alpha", Ping(2), cx), Ok(()));
    });
    assert_eq!(*log.borrow(), [("beta", 2)]);
}

#[gpui_kit::test]
fn undeclared_publishers_and_subscribers_are_refused(cx: &mut TestAppContext) {
    let key = MessageKey::event::<Ping>();
    cx.update(|cx| {
        assert!(matches!(
            publish("alpha", Ping(1), cx),
            Err(ContractError::Undeclared { .. })
        ));
        install(vec![("alpha", PUBLISHER), ("beta", SUBSCRIBER)], cx);
        assert_eq!(
            publish("beta", Ping(1), cx),
            Err(ContractError::Undeclared {
                board: "beta",
                role: Role::Publishes,
                key
            })
        );
        assert_eq!(
            publish("stopped", Ping(1), cx),
            Err(ContractError::Undeclared {
                board: "stopped",
                role: Role::Publishes,
                key
            })
        );
        let entity = cx.new(|_| Receiver);
        let refused = subscribe("alpha", &entity, |_, _: &Ping, _| {}, cx);
        assert_eq!(
            refused,
            Err(ContractError::Undeclared {
                board: "alpha",
                role: Role::Subscribes,
                key
            })
        );
        assert_eq!(receivers(cx), 0);
    });
}

#[gpui_kit::test]
fn a_cycle_of_events_stops_at_the_depth_limit(cx: &mut TestAppContext) {
    let refusals = Rc::new(RefCell::new(Vec::new()));
    let received = Rc::new(RefCell::new(0));
    let _receivers = cx.update(|cx| {
        install(vec![("alpha", PING_PONG), ("beta", PING_PONG)], cx);
        ["alpha", "beta"].map(|board| {
            let entity = cx.new(|_| Receiver);
            let (refusals, received) = (refusals.clone(), received.clone());
            let echo =
                move |_: &mut Receiver, ping: &Ping, cx: &mut gpui_kit::Context<Receiver>| {
                    *received.borrow_mut() += 1;
                    if let Err(error) = publish(board, Ping(ping.0 + 1), cx) {
                        refusals.borrow_mut().push(error);
                    }
                };
            assert_eq!(subscribe(board, &entity, echo, cx), Ok(()));
            entity
        })
    });
    cx.update(|cx| assert_eq!(publish("alpha", Ping(0), cx), Ok(())));
    assert_eq!(*received.borrow(), MAX_DEPTH + 1);
    assert!(matches!(
        refusals.borrow().as_slice(),
        [ContractError::TooDeep { .. }]
    ));
    cx.update(|cx| assert_eq!(publish("alpha", Ping(0), cx), Ok(())));
    assert_eq!(*received.borrow(), 2 * (MAX_DEPTH + 1), "depth resets");
}
