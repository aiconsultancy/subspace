use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{AppContext as _, TestAppContext};
use subspace_contracts::{MessageKey, Request};

use super::super::{BoardContracts, ContractError, Declaration, install, request, respond};
use super::{ANSWERER, ASKER, Double, Fixture};

struct Doubler;

#[gpui_kit::test]
fn requests_reach_the_running_responder_or_are_unavailable(cx: &mut TestAppContext) {
    let unavailable = Err(ContractError::Unavailable {
        key: MessageKey::request::<Double>(),
    });
    let responder = cx.update(|cx| {
        install(vec![("alpha", ASKER)], cx);
        assert_eq!(request("alpha", &Double(2), cx), unavailable);
        install(vec![("alpha", ASKER), ("beta", ANSWERER)], cx);
        assert_eq!(
            request("alpha", &Double(2), cx),
            unavailable,
            "no responder yet"
        );
        let entity = cx.new(|_| Doubler);
        let answered = respond(
            "beta",
            &entity,
            |_: &mut Doubler, ask: &Double, _| ask.0 * 2,
            cx,
        );
        assert_eq!(answered, Ok(()));
        assert_eq!(request("alpha", &Double(2), cx), Ok(4));
        assert!(matches!(
            request("gamma", &Double(2), cx),
            Err(ContractError::Undeclared { .. })
        ));
        install(vec![("alpha", ASKER)], cx);
        assert_eq!(
            request("alpha", &Double(2), cx),
            unavailable,
            "board stopped"
        );
        install(vec![("alpha", ASKER), ("beta", ANSWERER)], cx);
        entity.downgrade()
    });
    cx.update(|cx| {
        assert_eq!(
            request("alpha", &Double(2), cx),
            unavailable,
            "responder gone"
        )
    });
    assert!(responder.upgrade().is_none());
}

struct Triple(u32);

impl Request for Triple {
    type Contract = Fixture;
    const NAME: &'static str = "triple";
    type Response = u32;
}

#[gpui_kit::test]
fn a_cycle_of_requests_finds_the_first_responder_unavailable(cx: &mut TestAppContext) {
    const ALPHA: BoardContracts = BoardContracts::new(&[
        Declaration::responds::<Triple>(),
        Declaration::requests::<Double>(),
    ]);
    const BETA: BoardContracts = BoardContracts::new(&[
        Declaration::responds::<Double>(),
        Declaration::requests::<Triple>(),
    ]);
    const GAMMA: BoardContracts = BoardContracts::new(&[Declaration::requests::<Triple>()]);
    let inner = Rc::new(RefCell::new(None));
    let answer = cx.update(|cx| {
        install(vec![("alpha", ALPHA), ("beta", BETA), ("gamma", GAMMA)], cx);
        let alpha = cx.new(|_| Doubler);
        let beta = cx.new(|_| Doubler);
        let asks_beta = |_: &mut Doubler, ask: &Triple, cx: &mut gpui_kit::Context<Doubler>| {
            request("alpha", &Double(ask.0), cx).map_or(0, |doubled| doubled + ask.0)
        };
        let seen = inner.clone();
        let asks_alpha =
            move |_: &mut Doubler, ask: &Double, cx: &mut gpui_kit::Context<Doubler>| {
                *seen.borrow_mut() = Some(request("beta", &Triple(ask.0), cx));
                ask.0 * 2
            };
        assert_eq!(respond("alpha", &alpha, asks_beta, cx), Ok(()));
        assert_eq!(respond("beta", &beta, asks_alpha, cx), Ok(()));
        request("gamma", &Triple(3), cx)
    });
    assert_eq!(answer, Ok(9));
    assert_eq!(
        *inner.borrow(),
        Some(Err(ContractError::Unavailable {
            key: MessageKey::request::<Triple>()
        }))
    );
}
