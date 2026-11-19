use super::IssuesSettings;
use crate::{link, sync};
use gpui_kit::Context;
use std::time::Duration;
const POLL: Duration = Duration::from_millis(250);
impl IssuesSettings {
    pub(crate) fn start_polling(&mut self, cx: &mut Context<Self>) {
        self._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.poll_hub(cx)).is_err() {
                    return;
                }
            }
        }));
    }
    pub(crate) fn poll_hub(&mut self, cx: &mut Context<Self>) {
        let link = link::current(cx);
        if link.as_ref().map(|link| link.id) != self.hub_id {
            self.hub_id = link.as_ref().map(|link| link.id);
            self.address = link
                .as_ref()
                .and_then(|link| link.address.clone())
                .unwrap_or_else(super::api_address);
            self.hub = link.map(|link| link.hub);
            self.access = crate::scope::Access::Starting;
            self.seen = None;
            cx.notify();
        }
        let Some(hub) = &self.hub else {
            return;
        };
        if self.seen == Some(hub.version()) {
            return;
        }
        self.seen = Some(hub.version());
        let snapshot = hub.snapshot();
        self.access = match snapshot.status() {
            subspace_api_client::hub::HubStatus::Error(error) => {
                crate::scope::Access::Failed(sync::failure(error))
            }
            _ => sync::access(snapshot.status(), snapshot.me()),
        };
        cx.notify();
    }
}
