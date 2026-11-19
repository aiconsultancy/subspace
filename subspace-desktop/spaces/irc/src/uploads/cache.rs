use super::session::{Demand, Scope, Session};
use crate::hub::Hub;
use gpui_kit::{AppContext as _, Context, RenderImage};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use subspace_api_client::chat_uploads::Upload;
const RGBA_CHANNELS: usize = 4;
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);
use subspace_framework::dashboard::images::{DecodeLimits, decode_with_limits};
const MAX_CARDS: usize = 64;
const MAX_PIXELS_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone)]
pub(crate) enum Card {
    Loading,
    Ready(Arc<Upload>, Option<Arc<RenderImage>>),
    Expired,
    /// Deleted or never there: unavailable for good.
    Missing,
    /// Couldn't load now (network, sign-in): opening it tries again.
    Failed,
}
#[derive(Default)]
pub(crate) struct References {
    pub(crate) session: Session,
    pub(crate) demand: Demand,
    cards: HashMap<String, Card>,
    order: VecDeque<String>,
    queue: VecDeque<String>,
    busy: usize,
    pending: HashSet<String>,
    checked: HashMap<String, std::time::Instant>,
}
impl References {
    /// A card as if it had loaded, for tests.
    #[cfg(test)]
    pub(crate) fn test_insert(&mut self, key: String, card: Card) {
        self.insert(key, card);
    }
    pub(crate) fn has_cards(&self) -> bool {
        !self.cards.is_empty()
    }
    pub(crate) fn get(&self, key: &str) -> Option<&Card> {
        self.session.scope()?;
        self.cards.get(key)
    }
    pub(crate) fn clear(&mut self) {
        self.cards.clear();
        self.order.clear();
        self.queue.clear();
        self.checked.clear();
        self.pending.clear();
    }
    fn bytes(&self) -> usize {
        self.cards
            .values()
            .map(|c| match c {
                Card::Ready(_, Some(image)) => {
                    let size = image.size(0);
                    size.width.0 as usize * size.height.0 as usize * RGBA_CHANNELS
                }
                _ => 0,
            })
            .sum()
    }
    fn insert(&mut self, key: String, card: Card) {
        self.checked.insert(key.clone(), std::time::Instant::now());
        self.cards.insert(key, card);
        for old in &self.order {
            if self.bytes() <= MAX_PIXELS_BYTES {
                break;
            }
            if let Some(Card::Ready(_, image)) = self.cards.get_mut(old) {
                *image = None;
            }
        }
    }
}
pub(crate) fn want(hub: &mut Hub, keys: Vec<String>, cx: &mut Context<Hub>) {
    hub.uploads.demand.activate();
    if hub.uploads.session.scope().is_none() {
        return;
    }
    for key in keys {
        if hub.uploads.pending.contains(&key)
            || hub
                .uploads
                .checked
                .get(&key)
                .is_some_and(|at| at.elapsed() < CACHE_TTL)
        {
            continue;
        }
        hub.uploads.order.retain(|old| old != &key);
        while !hub.uploads.cards.contains_key(&key) && hub.uploads.cards.len() >= MAX_CARDS {
            if let Some(old) = hub.uploads.order.pop_front() {
                hub.uploads.cards.remove(&old);
                hub.uploads.checked.remove(&old);
                hub.uploads.queue.retain(|key| key != &old);
            } else {
                break;
            }
        }
        hub.uploads
            .cards
            .entry(key.clone())
            .or_insert(Card::Loading);
        hub.uploads.pending.insert(key.clone());
        hub.uploads.order.push_back(key.clone());
        hub.uploads.queue.push_back(key);
    }
    start(hub, cx);
}
fn start(hub: &mut Hub, cx: &mut Context<Hub>) {
    let Some(scope) = hub.uploads.session.scope() else {
        return;
    };
    while hub.uploads.busy < 2 {
        let Some(key) = hub.uploads.queue.pop_front() else {
            break;
        };
        hub.uploads.busy += 1;
        let scope = scope.clone();
        let previous = hub.uploads.cards.get(&key).cloned();
        cx.spawn(async move |this, cx| {
            let requested = key.clone();
            let read_scope = scope.clone();
            let card = cx
                .background_spawn(async move { load(&requested, &read_scope, previous) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.uploads.busy = hub.uploads.busy.saturating_sub(1);
                if hub.uploads.session.matches(&scope) && hub.uploads.cards.contains_key(&key) {
                    hub.uploads.pending.remove(&key);
                    hub.uploads.insert(key, card);
                }
                start(hub, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
fn load(key: &str, scope: &Scope, previous: Option<Card>) -> Card {
    let Ok(client) = scope.client() else {
        return Card::Failed;
    };
    let Ok(card) = client.resolve_chat_upload(key) else {
        return Card::Failed;
    };
    if card.status == "expired" {
        return Card::Expired;
    }
    let Some(meta) = card.upload else {
        return Card::Missing;
    };
    let held = match previous {
        Some(Card::Ready(old, image)) if old.id == meta.id && old.sha256 == meta.sha256 => image,
        _ => None,
    };
    let image = if held.is_some() {
        held
    } else if meta.content_type.starts_with("image/") {
        client
            .chat_upload_bytes(&meta)
            .ok()
            .and_then(|bytes| decode_with_limits(&bytes, &meta.content_type, limits()).ok())
    } else {
        None
    };
    if scope.client().is_err() {
        return Card::Failed;
    }
    Card::Ready(Arc::new(meta), image)
}
pub(crate) fn limits() -> DecodeLimits {
    DecodeLimits {
        encoded_bytes: subspace_api_client::chat_uploads::MAX_BYTES,
        dimension: 4096,
        pixels: 16_777_216,
    }
}
