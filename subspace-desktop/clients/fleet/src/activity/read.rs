//! Reading an officer's turns from the snapshot route, and an item too
//! large to send inline page by page: joined in full when the reader can
//! hold it, whatever its size, or else part by part, one page at a time.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize as _;
use serde_json::Value;
use url::Url;

use super::{ActivityEntry, ActivityItem, AgentActivity, ItemPage, UnknownItem};
use crate::{Error, FleetClient};

/// Sixteen turns of settled JSON stay below Stargate's 64 MiB of settled
/// turns, plus the running ones. Larger is an invalid answer, never cut.
/// An item has no such limit: its reader says how much it can hold.
const MAX_ACTIVITY_BYTES: u64 = 128 * 1024 * 1024;
/// One page holds at most 64 KiB of UTF-8; JSON escaping can make each
/// byte six, plus the page's own fields.
const MAX_PAGE_BYTES: u64 = 512 * 1024;
/// The most UTF-8 bytes in one page's `data`.
const PAGE_DATA_BYTES: u64 = 64 * 1024;
/// How many times a changing item is read again from page 0 before the
/// read gives up for now.
const PAGE_ATTEMPTS: usize = 5;

// qual:allow(coupling, oi) reason: "The activity reads compose the query encoder with the existing credential-bound transport."
impl FleetClient {
    /// `GET /api/v1/agents/{agent}/activity`, or one turn of it. Run in the
    /// background. `NotFound` means a gateway without the route, an
    /// unknown agent or, with `turn`, a turn the gateway no longer keeps.
    pub fn activity(&self, agent: &str, turn: Option<&str>) -> Result<AgentActivity, Error> {
        let mut url = self.activity_url(agent)?;
        if let Some(turn) = turn {
            url.query_pairs_mut().append_pair("turn", turn);
        }
        let activity: AgentActivity = self.get_json(&url, MAX_ACTIVITY_BYTES)?;
        if activity.agent.id != agent {
            return Err(Error::InvalidResponse);
        }
        Ok(activity)
    }

    /// One page of one item's JSON. Run in the background.
    pub fn activity_item_page(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
        page: u32,
    ) -> Result<ItemPage, Error> {
        let mut url = self.activity_url(agent)?;
        url.query_pairs_mut()
            .append_pair("turn", turn)
            .append_pair("item", item)
            .append_pair("page", &page.to_string());
        let answer: ItemPage = self.get_json(&url, MAX_PAGE_BYTES)?;
        let matches = answer.agent == agent
            && answer.turn == turn
            && answer.item == item
            && answer.page == page
            && answer.page < answer.pages
            && answer.data.len() as u64 <= PAGE_DATA_BYTES;
        matches.then_some(answer).ok_or(Error::InvalidResponse)
    }

    /// The complete item, page by page, as [`join_pages`] joins it and
    /// [`decode_item`] reads it, when it is at most `limit` bytes; page 0
    /// alone when it is larger, to read on in parts
    /// ([`Self::activity_item_part`]). Run in the background. `None` once
    /// `cancel` is set: it is checked before each page is requested, so no
    /// page is requested after it.
    pub fn activity_item(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
        limit: u64,
        cancel: &AtomicBool,
    ) -> Option<Result<ItemRead, Error>> {
        let joined = join_pages(cancel, limit, |page| {
            self.activity_item_page(agent, turn, item, page)
        })?;
        Some(joined.and_then(|joined| match joined {
            Joined::Whole { json, seq } => Ok(ItemRead::Whole {
                bytes: json.len() as u64,
                entry: decode_item(&json, item, seq)?,
            }),
            Joined::Larger(first) => Ok(ItemRead::Larger(first)),
        }))
    }

    /// Page `page` of an item too large to hold whole, of the version
    /// `pinned` (a page read before); page 0 of the newest version instead
    /// when the item changed since, or shrank so `page` is past its last,
    /// at most once. Run in the background;
    /// `None` once `cancel` is set, checked before each page is requested.
    pub fn activity_item_part(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
        (page, pinned): (u32, Option<&ItemPage>),
        cancel: &AtomicBool,
    ) -> Option<Result<ItemPart, PartFailed>> {
        read_part(cancel, page, pinned, |page| {
            self.activity_item_page(agent, turn, item, page)
        })
    }

    fn activity_url(&self, agent: &str) -> Result<Url, Error> {
        self.address().url(&["agents", agent, "activity"])
    }
}

/// What reading an item gave ([`FleetClient::activity_item`]).
#[non_exhaustive]
pub enum ItemRead {
    /// The item whole, `bytes` of JSON: of a kind or shape this version
    /// knows, or kept as its JSON ([`ActivityEntry::Unknown`]).
    Whole { entry: ActivityEntry, bytes: u64 },
    /// Page 0 of an item larger than the reader may hold: read it on part
    /// by part ([`FleetClient::activity_item_part`]).
    Larger(ItemPage),
}

/// One page of an item read part by part
/// ([`FleetClient::activity_item_part`]).
#[non_exhaustive]
pub struct ItemPart {
    pub page: ItemPage,
    /// The item changed since the page it was pinned to: this is page 0 of
    /// its newest version.
    pub newer: bool,
}

/// A part read that failed: why, and the page to ask for when it is tried
/// again ([`FleetClient::activity_item_part`]).
#[non_exhaustive]
pub struct PartFailed {
    pub page: u32,
    pub error: Error,
}

/// What [`join_pages`] joined. No `Debug`: it holds what a tool printed.
pub(crate) enum Joined {
    /// The whole item's JSON, at the `seq` its pages carried.
    Whole { json: String, seq: u64 },
    /// Page 0 of an item larger than the limit, read no further.
    Larger(ItemPage),
}

/// The complete JSON of item `item` at `seq`: of a kind and shape this
/// version knows, or else kept whole as its JSON, as an inline item of an
/// unknown kind or shape is, so it can always be shown. Its own ID and
/// `seq` must be the ones its pages carried, or the answer is invalid.
pub(crate) fn decode_item(json: &str, item: &str, seq: u64) -> Result<ActivityEntry, Error> {
    // Parsed once: the item is tried as a known kind from the parsed JSON,
    // which is kept whole when it is not one.
    let value: Value = serde_json::from_str(json).map_err(|_| Error::InvalidResponse)?;
    let entry = match ActivityItem::deserialize(&value) {
        Ok(known) => ActivityEntry::Item(Box::new(known)),
        Err(_) => ActivityEntry::Unknown(
            UnknownItem::deserialize(value).map_err(|_| Error::InvalidResponse)?,
        ),
    };
    let base = entry.base();
    if base.id == item && base.seq == seq {
        Ok(entry)
    } else {
        Err(Error::InvalidResponse)
    }
}

/// Why a join stopped short of the whole item.
enum Stop {
    Failed(Error),
    Cancelled,
}

/// Joins the pages `read` gives into the item's complete JSON, unless page
/// 0 declares more than `limit` bytes: then page 0 alone, and no other
/// page is read. A page of another `seq` or page count belongs to another
/// version of the item: start again from page 0, at most [`PAGE_ATTEMPTS`]
/// times. Nothing is ever cut: a joined length that is not the item's
/// `bytes` is an invalid answer. `None` once `cancel` is set, checked
/// before every read.
pub(crate) fn join_pages(
    cancel: &AtomicBool,
    limit: u64,
    mut read: impl FnMut(u32) -> Result<ItemPage, Error>,
) -> Option<Result<Joined, Error>> {
    let mut read = |page| {
        if cancel.load(Ordering::Acquire) {
            Err(Stop::Cancelled)
        } else {
            read(page).map_err(Stop::Failed)
        }
    };
    for _ in 0..PAGE_ATTEMPTS {
        match whole(&mut read, limit) {
            Ok(Some(joined)) => return Some(Ok(joined)),
            Ok(None) => {}
            Err(Stop::Failed(error)) => return Some(Err(error)),
            Err(Stop::Cancelled) => return None,
        }
    }
    Some(Err(Error::Busy(None)))
}

/// One attempt at the whole item from page 0; `None` when it changed.
fn whole(
    read: &mut impl FnMut(u32) -> Result<ItemPage, Stop>,
    limit: u64,
) -> Result<Option<Joined>, Stop> {
    let first = read(0)?;
    if !sound(&first) {
        return Err(Stop::Failed(Error::InvalidResponse));
    }
    if first.bytes > limit {
        return Ok(Some(Joined::Larger(first)));
    }
    let seq = first.seq;
    Ok(rest_of(&first, read)?.map(|json| Joined::Whole { json, seq }))
}

/// Whether a page declares a size this client reads, whatever its size:
/// at least one page, no more bytes than its pages hold, no more pages
/// than bytes, since each page carries some, and data within the item,
/// none of it empty.
fn sound(page: &ItemPage) -> bool {
    let pages = u64::from(page.pages);
    pages >= 1
        && page.bytes <= pages.saturating_mul(PAGE_DATA_BYTES)
        && pages <= page.bytes.max(1)
        && page.data.len() as u64 <= page.bytes
        && (page.bytes == 0 || !page.data.is_empty())
}

/// The pages after `first` joined to it; `None` when the item changed.
/// Nothing is reserved from the declared size: the JSON grows only as
/// pages arrive, and never past `bytes`.
fn rest_of(
    first: &ItemPage,
    read: &mut impl FnMut(u32) -> Result<ItemPage, Stop>,
) -> Result<Option<String>, Stop> {
    let invalid = || Stop::Failed(Error::InvalidResponse);
    let mut json = first.data.clone();
    for page in 1..first.pages {
        let next = read(page)?;
        if !same_version(&next, first) {
            return Ok(None);
        }
        if next.data.is_empty() || (json.len() + next.data.len()) as u64 > first.bytes {
            return Err(invalid());
        }
        json.push_str(&next.data);
    }
    if json.len() as u64 == first.bytes {
        Ok(Some(json))
    } else {
        Err(invalid())
    }
}

/// Whether two pages belong to the same version of the item.
fn same_version(page: &ItemPage, other: &ItemPage) -> bool {
    page.seq == other.seq && page.pages == other.pages && page.bytes == other.bytes
}

/// Page `page` of the version `pinned`, or, when the item changed since,
/// page 0 of its newest version: read again once, never more. A page past
/// the item's last (404) means it shrank, so it changed too. A failure
/// names the page to try again: page 0 once the item is known to have
/// changed, never the old index.
pub(crate) fn read_part(
    cancel: &AtomicBool,
    page: u32,
    pinned: Option<&ItemPage>,
    mut read: impl FnMut(u32) -> Result<ItemPage, Error>,
) -> Option<Result<ItemPart, PartFailed>> {
    let mut read = |page| {
        if cancel.load(Ordering::Acquire) {
            return None;
        }
        Some(read(page).and_then(|answer| {
            if sound(&answer) {
                Ok(answer)
            } else {
                Err(Error::InvalidResponse)
            }
        }))
    };
    let changed = match read(page)? {
        Ok(answer) if pinned.is_none_or(|pinned| same_version(&answer, pinned)) => {
            return Some(Ok(ItemPart {
                page: answer,
                newer: false,
            }));
        }
        // Changed, and this is already page 0 of the newer version.
        Ok(answer) if page == 0 => {
            return Some(Ok(ItemPart {
                page: answer,
                newer: true,
            }));
        }
        Ok(_) => true,
        Err(Error::NotFound) if page > 0 => true,
        Err(error) => return Some(Err(PartFailed { page, error })),
    };
    Some(match read(0)? {
        Ok(first) => Ok(ItemPart {
            page: first,
            newer: changed,
        }),
        Err(error) => Err(PartFailed { page: 0, error }),
    })
}
