//! Reading an officer's turns from the snapshot route, and an item too
//! large to send inline page by page, joined in full.

use std::sync::atomic::{AtomicBool, Ordering};

use url::Url;

use super::{ActivityItem, AgentActivity, ItemPage};
use crate::{Error, FleetClient};

/// Sixteen turns of settled JSON stay below Stargate's 64 MiB of settled
/// turns, plus the running ones. Larger is an invalid answer, never cut.
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

    /// Every page of one item joined into its complete JSON, read again
    /// from page 0 whenever the item changed between pages. Run in the
    /// background. `None` once `cancel` is set: it is checked before each
    /// page is requested, so no page is requested after it.
    pub fn activity_item_json(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
        cancel: &AtomicBool,
    ) -> Option<Result<String, Error>> {
        join_pages(cancel, |page| {
            self.activity_item_page(agent, turn, item, page)
        })
    }

    /// The complete item, read page by page ([`Self::activity_item_json`]);
    /// `None` once `cancel` is set.
    pub fn activity_item(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
        cancel: &AtomicBool,
    ) -> Option<Result<ActivityItem, Error>> {
        let json = self.activity_item_json(agent, turn, item, cancel)?;
        Some(json.and_then(|json| serde_json::from_str(&json).map_err(|_| Error::InvalidResponse)))
    }

    fn activity_url(&self, agent: &str) -> Result<Url, Error> {
        self.address().url(&["agents", agent, "activity"])
    }
}

/// Why a join stopped short of the whole item.
enum Stop {
    Failed(Error),
    Cancelled,
}

/// Joins the pages `read` gives into the item's complete JSON. A page of
/// another `seq` or page count belongs to another version of the item:
/// start again from page 0, at most [`PAGE_ATTEMPTS`] times. Nothing is
/// ever cut: a joined length that is not the item's `bytes` is an invalid
/// answer. `None` once `cancel` is set, checked before every read.
pub(crate) fn join_pages(
    cancel: &AtomicBool,
    mut read: impl FnMut(u32) -> Result<ItemPage, Error>,
) -> Option<Result<String, Error>> {
    let mut read = |page| {
        if cancel.load(Ordering::Acquire) {
            Err(Stop::Cancelled)
        } else {
            read(page).map_err(Stop::Failed)
        }
    };
    for _ in 0..PAGE_ATTEMPTS {
        match whole(&mut read) {
            Ok(Some(json)) => return Some(Ok(json)),
            Ok(None) => {}
            Err(Stop::Failed(error)) => return Some(Err(error)),
            Err(Stop::Cancelled) => return None,
        }
    }
    Some(Err(Error::Busy(None)))
}

/// One attempt at the whole item from page 0; `None` when it changed.
fn whole(read: &mut impl FnMut(u32) -> Result<ItemPage, Stop>) -> Result<Option<String>, Stop> {
    let first = read(0)?;
    rest_of(&first, read)
}

/// Whether page 0 declares a size this client reads: at least one page,
/// no more bytes than its pages hold or than [`MAX_ACTIVITY_BYTES`], and
/// no more pages than bytes, since each page carries some.
fn plausible(first: &ItemPage) -> bool {
    let pages = u64::from(first.pages);
    pages >= 1
        && first.bytes <= pages.saturating_mul(PAGE_DATA_BYTES)
        && first.bytes <= MAX_ACTIVITY_BYTES
        && pages <= first.bytes.max(1)
}

/// The pages after `first` joined to it; `None` when the item changed.
/// Nothing is reserved from the declared size: the JSON grows only as
/// pages arrive, and never past `bytes`.
fn rest_of(
    first: &ItemPage,
    read: &mut impl FnMut(u32) -> Result<ItemPage, Stop>,
) -> Result<Option<String>, Stop> {
    let invalid = || Stop::Failed(Error::InvalidResponse);
    if !plausible(first) || first.data.len() as u64 > first.bytes {
        return Err(invalid());
    }
    let mut json = first.data.clone();
    for page in 1..first.pages {
        let next = read(page)?;
        if next.seq != first.seq || next.pages != first.pages || next.bytes != first.bytes {
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
