//! One retained Subspace API hub, shared by the board and its responder.
use crate::{
    Issues,
    responder::{self, Responder},
};
use gpui_kit::{App, Context, Entity, Global, Subscription};
use std::sync::atomic::{AtomicU64, Ordering};
use subspace_api_client::hub::Hub;
use subspace_framework::core::log::Logger;
static STARTS: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) hub: Hub,
    pub(crate) id: u64,
    pub(crate) address: Option<String>,
}
struct IssuesHub {
    link: Link,
    responder: Entity<Responder>,
}
impl Global for IssuesHub {}
pub(crate) fn current(cx: &App) -> Option<Link> {
    cx.try_global::<IssuesHub>()
        .map(|global| global.link.clone())
}
pub(crate) fn responder(cx: &App) -> Option<Entity<Responder>> {
    cx.try_global::<IssuesHub>()
        .map(|global| global.responder.clone())
}
pub(crate) fn observe(cx: &mut Context<Issues>) -> Subscription {
    cx.observe_global::<IssuesHub>(|board, cx| board.poll_hub(cx))
}
pub(crate) fn start(cx: &mut App) {
    if cx.has_global::<IssuesHub>() {
        return;
    }
    #[cfg(debug_assertions)]
    if let Some((hub, address)) = crate::fixture::from_env() {
        install(cx, hub, Some(address));
        return;
    }
    match Hub::start() {
        Ok(hub) => install(cx, hub, None),
        Err(error) => Logger::new(crate::ID).warn(format!("The Issues hub didn't start: {error}")),
    }
}
pub(crate) fn install(cx: &mut App, hub: Hub, address: Option<String>) {
    let link = Link {
        hub,
        id: STARTS.fetch_add(1, Ordering::Relaxed),
        address,
    };
    let responder = responder::attach(link.hub.clone(), cx);
    cx.set_global(IssuesHub { link, responder });
}
pub(crate) fn stop(cx: &mut App) {
    if let Some(link) = current(cx) {
        responder::announce(cx);
        link.hub.stop();
    }
    if cx.has_global::<IssuesHub>() {
        cx.remove_global::<IssuesHub>();
    }
}
