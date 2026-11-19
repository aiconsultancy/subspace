//! Routes each server message to its handler.

use super::{
    Output, Session, batch, caps, channel_info, listing, member_state, membership, messages,
    numerics, registration,
};
use crate::proto::Message;

pub(super) fn handle(session: &mut Session, message: &Message, out: &mut Output) {
    match message.command.as_str() {
        "PING" => registration::ping(session, message, out),
        "PONG" | "CHGHOST" | "SETNAME" | "908" => {}
        "ERROR" => registration::error(session, message, out),
        "CAP" => caps::cap(session, message, out),
        "AUTHENTICATE" => caps::authenticate(session, message, out),
        "BATCH" => batch::batch(session, message, out),
        "PRIVMSG" => messages::privmsg(session, message, out),
        "NOTICE" => messages::notice(session, message, out),
        "TAGMSG" => messages::tagmsg(session, message, out),
        "MARKREAD" => messages::markread(session, message, out),
        "FAIL" | "WARN" | "NOTE" if message.param(0).eq_ignore_ascii_case("METADATA") => {
            super::metadata::reply(session, message, out)
        }
        "FAIL" | "WARN" | "NOTE" => messages::standard_reply(session, message, out),
        "METADATA" | "761" | "766" => super::metadata::receive(session, message, out),
        "760" | "762" | "763" | "764" | "765" | "767" | "768" | "769" | "770" | "771" | "772"
        | "773" => super::metadata::reply(session, message, out),
        "774" => super::metadata::sync_later(session, message, out),
        "421" | "461" if message.param(1).eq_ignore_ascii_case("METADATA") => {
            super::metadata::reply(session, message, out)
        }
        "JOIN" => membership::join(session, message, out),
        "PART" => membership::part(session, message, out),
        "KICK" => membership::kick(session, message, out),
        "QUIT" => membership::quit(session, message, out),
        "NICK" => membership::nick(session, message, out),
        "MODE" => member_state::mode(session, message, out),
        "TOPIC" => channel_info::topic(session, message, out),
        "INVITE" => member_state::invite(session, message, out),
        "ACCOUNT" => member_state::account(session, message, out),
        "AWAY" => member_state::away(session, message, out),
        "001" => registration::welcome(session, message, out),
        "005" => numerics::isupport(session, message, out),
        "221" => numerics::user_modes(session, message, out),
        "321" => listing::start(session, message, out),
        "322" => listing::entry(session, message, out),
        "323" => listing::end(session, message, out),
        "376" | "422" => listing::motd_end(session, message, out),
        "331" => channel_info::no_topic(session, message, out),
        "332" => channel_info::topic_reply(session, message, out),
        "333" => channel_info::topic_who(session, message, out),
        "353" => channel_info::names(session, message, out),
        "366" => channel_info::end_of_names(session, message, out),
        "354" => channel_info::whox(session, message, out),
        "315" => channel_info::end_of_who(session, message, out),
        "432" | "433" | "436" | "437" => registration::nick_error(session, message, out),
        "900" => numerics::info(session, message, out),
        "903" | "907" => caps::sasl_done(session, message, out),
        "904" | "905" | "906" | "902" => caps::sasl_failed(session, message, out),
        _ => numerics::other(session, message, out),
    }
}
