//! `LIST`: the server's public channels. Each connection asks once after the
//! message of the day; the owner may ask again. Replies (`RPL_LIST`, 322)
//! collect until `RPL_LISTEND` (323), which reports them all at once. At
//! most one `LIST` runs per connection.

use super::{Output, Session, numerics};
use crate::model::{Event, ListedChannel};
use crate::proto::Message;

const LIST: &str = "LIST";

/// The `LIST` of one connection.
#[derive(Debug, Default)]
pub struct Listing {
    /// A `LIST` we sent has not ended yet.
    running: bool,
    /// This connection already asked by itself.
    asked: bool,
    collected: Vec<ListedChannel>,
}

impl Listing {
    /// Starts a `LIST` when `registered` and none runs. True when it should
    /// be sent.
    fn begin(&mut self, registered: bool) -> bool {
        let start = registered && !self.running;
        if start {
            self.running = true;
            self.collected.clear();
        }
        start
    }

    /// Starts the connection's own `LIST`, once. True when it should be sent.
    fn begin_once(&mut self) -> bool {
        let start = !self.asked && !self.running;
        self.asked = true;
        if start {
            self.running = true;
            self.collected.clear();
        }
        start
    }

    /// Ends the running `LIST` without a result when a refusal names
    /// `command` and it is `LIST`.
    fn refuse(&mut self, refusal: bool, command: &str) {
        if refusal && command.eq_ignore_ascii_case(LIST) {
            self.running = false;
            self.collected.clear();
        }
    }

    /// Ends the `LIST` with what it collected.
    fn finish(&mut self) -> Vec<ListedChannel> {
        self.running = false;
        std::mem::take(&mut self.collected)
    }
}

fn list() -> Message {
    Message::new(LIST, Vec::<String>::new())
}

/// `Command::ListChannels`: sends `LIST` once registered, unless one runs.
pub(super) fn request(session: &mut Session, out: &mut Output) {
    let start = session.listing.begin(session.reg.registered);
    out.send.extend(start.then(list));
}

/// End of the message of the day (376) or none (422): the line, then the
/// connection's own `LIST`.
pub(super) fn motd_end(session: &mut Session, message: &Message, out: &mut Output) {
    numerics::other(session, message, out);
    let start = session.listing.begin_once();
    out.send.extend(start.then(list));
}

/// `RPL_LISTSTART` (321) says nothing the list does not.
pub(super) fn start(_: &mut Session, _: &Message, _: &mut Output) {}

/// `RPL_LIST` (322): `<nick> <channel> <users> :<topic>`. A name that is not
/// a channel, such as the `*` some servers send for a private one, is left
/// out.
pub(super) fn entry(session: &mut Session, message: &Message, _: &mut Output) {
    let channel = listed(&message.params, &session.isupport.chantypes);
    session.listing.collected.extend(channel);
}

/// `RPL_LISTEND` (323): reports what was collected, also for a `LIST` typed
/// with `/raw`.
pub(super) fn end(session: &mut Session, _: &Message, out: &mut Output) {
    let channels = session.listing.finish();
    out.event(Event::ChannelList {
        network: session.config.id.clone(),
        channels,
    });
}

/// A numeric naming `LIST` after our nick, such as `RPL_TRYAGAIN` (263) or
/// an unknown command: the server refused it, and the owner keeps its list.
pub(super) fn refused(session: &mut Session, message: &Message) {
    session.listing.refuse(true, message.param(1));
}

/// A `FAIL LIST …` standard reply ends the `LIST` as [`refused`] does.
pub(super) fn failed(session: &mut Session, message: &Message) {
    session
        .listing
        .refuse(message.command == "FAIL", message.param(0));
}

/// The channel of `RPL_LIST` parameters `<nick> <channel> <users> :<topic>`.
fn listed(params: &[String], chantypes: &[char]) -> Option<ListedChannel> {
    let [_, name, users, rest @ ..] = params else {
        return None;
    };
    let channel = name.len() > 1 && name.starts_with(chantypes);
    channel.then(|| ListedChannel {
        name: name.clone(),
        users: users.parse().unwrap_or(0),
        topic: rest.first().cloned().unwrap_or_default(),
    })
}
