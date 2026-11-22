//! One channel or private message as one record in the board's store, and
//! back. Pure data: no GPUI, no I/O.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use subspace_client_irc::{BufferKey, Line, NetworkId, Target, format};
use subspace_framework::dashboard::full_text::{Document, IndexMetadata, Scope};

use super::links;
use crate::state::is_message;

/// IRC keeps one local scope: messages belong to this computer's profile,
/// not to a service account. The network is the `project` filter.
const LOCAL: &str = "local";
pub(crate) const CHANNEL_KIND: &str = "Channel message";
pub(crate) const PRIVATE_KIND: &str = "Private message";
/// A request that ended, stored beside its buffer's lines.
pub(crate) const ENDING_KIND: &str = "Channels request ending";

/// The board's one search scope in app `app`.
pub(crate) fn scope(app: &str) -> Scope {
    Scope {
        app: app.to_owned(),
        board: crate::ID.to_owned(),
        endpoint: LOCAL.to_owned(),
        tenant: String::new(),
        account: LOCAL.to_owned(),
    }
}

/// Where a record leads: the buffer, and the line by `msgid` or time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Navigation {
    pub network: NetworkId,
    pub target: Target,
    pub msgid: Option<String>,
    pub time: DateTime<Utc>,
    /// The sender, for tools that list a hit without its record.
    #[serde(default)]
    pub from: Option<String>,
}

impl Navigation {
    pub fn buffer(&self) -> BufferKey {
        BufferKey {
            network: self.network.clone(),
            target: self.target.clone(),
        }
    }
}

/// Whether `line` is stored: a message in a channel or private conversation.
/// Membership, topic, mode and server lines are not.
pub(crate) fn stored(line: &Line) -> bool {
    line.buffer.target != Target::Server && is_message(line.kind)
}

/// The buffer's name as stored: ASCII lower case, as the runtime may spell
/// it differently after a restart.
fn folded(target: &Target) -> Option<(char, String)> {
    match target {
        Target::Channel(name) => Some(('c', name.to_ascii_lowercase())),
        Target::Query(nick) => Some(('q', nick.to_ascii_lowercase())),
        Target::Server => None,
    }
}

/// The record parent of buffer `buffer`: `buffer:<network>:<c|q>:<name>`.
pub(crate) fn parent_id(buffer: &BufferKey) -> String {
    let (kind, name) = folded(&buffer.target).unwrap_or(('s', String::new()));
    format!("buffer:{}:{kind}:{name}", buffer.network.0)
}

/// The record id of `line`: its `msgid`, unique per network, else a hash of
/// where, when, who and what, so the copies a server sends again dedupe.
pub(crate) fn document_id(line: &Line) -> String {
    if let Some(id) = &line.id {
        return format!("msgid:{}:{id}", line.buffer.network.0);
    }
    let fields = [
        parent_id(&line.buffer),
        line.time.timestamp_millis().to_string(),
        line.from.clone().unwrap_or_default().to_ascii_lowercase(),
        line.text.clone(),
    ];
    let digest = Sha256::digest(fields.join("\u{0}").as_bytes());
    format!("line:{}", hex::encode(digest))
}

/// The record of a stored line at `now`; `None` for lines that are not
/// stored. A server time ahead of this computer's clock is stored as now.
pub(crate) fn document(line: &Line, now: DateTime<Utc>) -> Option<Document> {
    if !stored(line) {
        return None;
    }
    let from = line.from.as_deref().unwrap_or_default();
    let (kind, title, target) = match &line.buffer.target {
        Target::Channel(name) => (CHANNEL_KIND, format!("{name} · {from}"), name),
        Target::Query(nick) => (PRIVATE_KIND, from.to_owned(), nick),
        Target::Server => return None,
    };
    // The fields of `Navigation`, serialised from the line in place.
    let navigation = serde_json::json!({
        "network": line.buffer.network,
        "target": line.buffer.target,
        "msgid": line.id,
        "time": line.time,
        "from": line.from,
    });
    Some(Document {
        id: document_id(line),
        title,
        body: format::strip(&line.text),
        metadata: format!("{from} {target}"),
        parent_id: Some(parent_id(&line.buffer)),
        kind: kind.to_owned(),
        url: None,
        navigation: navigation.to_string(),
        refreshed_at: line.time.min(now).timestamp(),
        payload: serde_json::to_string(line).ok(),
    })
}

/// The network filter and the GitHub items `line` links.
pub(crate) fn metadata(line: &Line) -> IndexMetadata {
    IndexMetadata {
        project: Some(line.buffer.network.0.clone()),
        references: links::references(&format::strip(&line.text)),
    }
}

/// The line a record holds, back from its payload. An ending record holds
/// none.
pub(crate) fn line(document: &Document) -> Option<Line> {
    let payload = document
        .payload
        .as_deref()
        .filter(|_| document.kind != ENDING_KIND)?;
    serde_json::from_str(payload).ok()
}

/// A request that completed or failed, by a trusted notice, as the store
/// keeps it: the buffer, so it restores without any surviving line, and the
/// request's message ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ended {
    pub buffer: BufferKey,
    pub request: String,
}

/// The record of `ended` at `now`: one per request, under its buffer's
/// parent, so removing the buffer deletes it with the lines. It has no
/// text, so no search finds it.
pub(crate) fn ending_document(ended: &Ended, now: DateTime<Utc>) -> Option<Document> {
    Some(Document {
        id: format!("ended:{}:{}", ended.buffer.network.0, ended.request),
        title: String::new(),
        body: String::new(),
        metadata: String::new(),
        parent_id: Some(parent_id(&ended.buffer)),
        kind: ENDING_KIND.to_owned(),
        url: None,
        navigation: String::new(),
        refreshed_at: now.timestamp(),
        payload: Some(serde_json::to_string(ended).ok()?),
    })
}

/// The ending an ending record holds.
pub(crate) fn ended(document: &Document) -> Option<Ended> {
    let payload = document
        .payload
        .as_deref()
        .filter(|_| document.kind == ENDING_KIND)?;
    serde_json::from_str(payload).ok()
}

/// Where a record leads, from its navigation.
pub(crate) fn navigation(document: &Document) -> Option<Navigation> {
    serde_json::from_str(&document.navigation).ok()
}
