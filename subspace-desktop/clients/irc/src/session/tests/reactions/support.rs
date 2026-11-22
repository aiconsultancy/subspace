//! Reaction support, destinations and ownership by account.

use super::super::{ALL_CAPS, PLAIN_CAPS, buffer, channel, command, config, feed, in_channel, now};
use super::{react, reactions, refusal};
use crate::model::{ReactionError, Target, Update};
use crate::session::{Carry, Output, Session};

#[test]
fn reactions_need_one_valid_destination() {
    let mut session = in_channel(ALL_CAPS);
    for target in [
        Target::Query(String::new()),
        Target::Query("#c".into()),
        Target::Query("a,b".into()),
        Target::Query("a b".into()),
        Target::Channel("c".into()),
        Target::Channel("#a,#b".into()),
    ] {
        let out = command(&mut session, react(buffer(target), "m1", "👍"));
        assert!(out.send.is_empty());
        assert_eq!(refusal(&out), Some(&ReactionError::InvalidTarget));
    }
}

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
fn clienttagdeny_turns_reactions_off_and_reports_it() {
    let mut session = in_channel(ALL_CAPS);
    let denied = feed(
        &mut session,
        &[":ergo.test 005 subspace CLIENTTAGDENY=*,-typing :are supported by this server"],
    );
    assert_eq!(support(&denied), [false]);
    let out = command(&mut session, react(channel("#c"), "m1", "👍"));
    assert!(out.send.is_empty());
    assert_eq!(refusal(&out), Some(&ReactionError::Unsupported));
    let exempt = feed(
        &mut session,
        &[
            ":ergo.test 005 subspace CLIENTTAGDENY=*,-draft/react,-draft/unreact :are supported by this server",
        ],
    );
    assert_eq!(support(&exempt), [true]);
    let unchanged = feed(
        &mut session,
        &[":ergo.test 005 subspace NICKLEN=32 :are supported"],
    );
    assert!(support(&unchanged).is_empty(), "reported only on a change");
}

#[test]
fn capability_reports_carry_reaction_support() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let out = feed(
        &mut session,
        &[
            ":ergo.test CAP * LS :batch",
            ":ergo.test CAP subspace ACK :batch",
        ],
    );
    assert_eq!(support(&out), [false]);
    let mut tagged = in_channel(ALL_CAPS);
    let out = feed(&mut tagged, &[":ergo.test CAP subspace DEL :message-tags"]);
    assert_eq!(support(&out), [false]);
}

#[test]
fn ownership_follows_our_account_when_both_are_known() {
    let mut session = in_channel(ALL_CAPS);
    feed(
        &mut session,
        &[":ergo.test 900 subspace subspace!u@h me-account :You are now logged in as me-account"],
    );
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +h draft/chathistory #c",
            "@batch=h;account=me-account;+draft/react=👍;+draft/reply=m1 :oldnick!u@h TAGMSG #c",
            "@batch=h;account=someone;+draft/react=👍;+draft/reply=m1 :subspace!x@h TAGMSG #c",
            "@batch=h;+draft/react=🎉;+draft/reply=m1 :subspace!x@h TAGMSG #c",
            ":ergo.test BATCH -h",
        ],
    );
    let owned: Vec<bool> = reactions(&out)
        .iter()
        .map(|reaction| reaction.own)
        .collect();
    assert_eq!(owned, [true, false, true], "accounts first, else our nick");
    let mut plain = in_channel(PLAIN_CAPS);
    feed(
        &mut plain,
        &[":ergo.test 900 subspace subspace!u@h me-account :You are now logged in as me-account"],
    );
    let echo = command(&mut plain, react(channel("#c"), "m1", "👍"));
    assert_eq!(reactions(&echo)[0].account.as_deref(), Some("me-account"));
}
