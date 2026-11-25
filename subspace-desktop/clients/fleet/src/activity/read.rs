//! Reading an officer's turns from the snapshot route, and an item too
//! large to send inline page by page, joined in full.

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
    /// background.
    pub fn activity_item_json(&self, agent: &str, turn: &str, item: &str) -> Result<String, Error> {
        join_pages(|page| self.activity_item_page(agent, turn, item, page))
    }

    /// The complete item, read page by page ([`Self::activity_item_json`]).
    pub fn activity_item(
        &self,
        agent: &str,
        turn: &str,
        item: &str,
    ) -> Result<ActivityItem, Error> {
        let json = self.activity_item_json(agent, turn, item)?;
        serde_json::from_str(&json).map_err(|_| Error::InvalidResponse)
    }

    fn activity_url(&self, agent: &str) -> Result<Url, Error> {
        self.address().url(&["agents", agent, "activity"])
    }
}

/// Joins the pages `read` gives into the item's complete JSON. A page of
/// another `seq` or page count belongs to another version of the item:
/// start again from page 0, at most [`PAGE_ATTEMPTS`] times. Nothing is
/// ever cut: a joined length that is not the item's `bytes` is an invalid
/// answer.
pub(crate) fn join_pages(
    mut read: impl FnMut(u32) -> Result<ItemPage, Error>,
) -> Result<String, Error> {
    for _ in 0..PAGE_ATTEMPTS {
        let first = read(0)?;
        if let Some(json) = rest_of(&first, &mut read)? {
            return Ok(json);
        }
    }
    Err(Error::Busy(None))
}

/// The pages after `first` joined to it; `None` when the item changed.
fn rest_of(
    first: &ItemPage,
    read: &mut impl FnMut(u32) -> Result<ItemPage, Error>,
) -> Result<Option<String>, Error> {
    if u64::from(first.pages) * PAGE_DATA_BYTES < first.bytes {
        return Err(Error::InvalidResponse);
    }
    let mut json = String::with_capacity(usize::try_from(first.bytes).unwrap_or_default());
    json.push_str(&first.data);
    for page in 1..first.pages {
        let next = read(page)?;
        if next.seq != first.seq || next.pages != first.pages || next.bytes != first.bytes {
            return Ok(None);
        }
        json.push_str(&next.data);
    }
    if json.len() as u64 == first.bytes {
        Ok(Some(json))
    } else {
        Err(Error::InvalidResponse)
    }
}
