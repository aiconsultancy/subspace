//! Support across registration and disconnection, history ownership and
//! destinations that would not fit one line.

use super::super::{ALL_CAPS, buffer, command, config, feed, in_channel, now};
use super::{react, reactions, refusal};
use crate::model::{ReactionError, Target, Update};
use crate::session::{Carry, Output, Session};

fn support(out: &Output) -> Vec<bool> {
    out.updates
        .now
        .iter()
        .filter_map(|update| match update {
            Update::ReactionSupport { supported, .. } => Some(*supported),
            _ => None,
        })
        .collect()
}

#[test]
fn support_waits_for_the_welcome_and_ends_with_the_connection() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let negotiated = feed(
        &mut session,
        &[
            ":ergo.test CAP * LS :message-tags batch",
            ":ergo.test CAP subspace ACK :message-tags batch",
        ],
    );
    assert!(
        !support(&negotiated).contains(&true),
        "not before registration"
    );
    let welcomed = feed(&mut session, &[":ergo.test 001 subspace :Welcome"]);
    assert_eq!(support(&welcomed), [true], "reported once welcomed");
    assert!(matches!(
        crate::session::reactions_ended(session.network()),
        Update::ReactionSupport {
            supported: false,
            ..
        }
    ));
}

#[test]
fn history_without_an_account_is_never_taken_as_ours_by_nick() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +h draft/chathistory #c",
            "@batch=h;+draft/react=🎉;+draft/reply=m1 :subspace!x@h TAGMSG #c",
            ":ergo.test BATCH -h",
            "@+draft/react=👍;+draft/reply=m1 :subspace!x@h TAGMSG #c",
        ],
    );
    let owned: Vec<bool> = reactions(&out)
        .iter()
        .map(|reaction| reaction.own)
        .collect();
    assert_eq!(
        owned,
        [false, true],
        "history needs our account; live traffic keeps the nick"
    );
}

#[test]
fn a_destination_that_would_not_fit_one_line_is_refused() {
    let mut session = in_channel(ALL_CAPS);
    let nick = "n".repeat(504);
    let out = command(&mut session, react(buffer(Target::Query(nick)), "m1", "👍"));
    assert!(out.send.is_empty(), "nothing is cut short on the wire");
    assert_eq!(refusal(&out), Some(&ReactionError::InvalidTarget));
    let fits = command(
        &mut session,
        react(buffer(Target::Query("n".repeat(503))), "m1", "👍"),
    );
    assert_eq!(fits.send.len(), 1);
}
