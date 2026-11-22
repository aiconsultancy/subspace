//! Runtime changes wake one retained observer; filesystem reads stay off the UI thread.
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AppContext as _, AsyncWindowContext, Context, WeakEntity, Window};

use super::ProjectBrowser;
use crate::project_service::ProjectService;

// About 30 Hz keeps live dashboards responsive without spending every UI frame
// rebuilding charts. Faster publications keep their state but share a repaint.
const REPAINT_BUDGET: Duration = Duration::from_millis(33);
const FALLBACK_INTERVAL: Duration = Duration::from_secs(1);

impl ProjectBrowser {
    pub(super) fn observe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.observation.is_some() {
            return;
        }
        let root = self.root.clone();
        self.observation = Some(cx.spawn_in(window, async move |this, cx| {
            let service = cx
                .background_spawn(async move { crate::project_service::shared(&root) })
                .await;
            let service = match service {
                Ok(service) => service,
                Err(error) => {
                    _ = this.update(cx, |this, cx| this.fail(error.to_string(), cx));
                    return;
                }
            };
            let mut changes = service.subscribe();
            loop {
                // The watch stores only "changed", not one queued item per patch.
                // The slow fallback also catches session exits and external edits.
                tokio::select! {
                    result = changes.changed() => if result.is_err() { break; },
                    _ = cx.background_executor().timer(FALLBACK_INTERVAL) => {},
                }
                // Do not poll the watch during this delay. A burst wakes us once,
                // then all publications in this budget share the latest readback.
                cx.background_executor().timer(REPAINT_BUDGET).await;
                changes.borrow_and_update();
                if !refresh_selected(&this, service.clone(), cx).await {
                    break;
                }
            }
        }));
    }
}

async fn refresh_selected(
    browser: &WeakEntity<ProjectBrowser>,
    service: Arc<ProjectService>,
    cx: &mut AsyncWindowContext,
) -> bool {
    let selection = browser.update(cx, |this, _| {
        (!this.busy).then(|| this.selected_id()).flatten()
    });
    let id = match selection {
        Ok(Some(id)) => id,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let selected = id.clone();
    let result = cx
        .background_spawn(async move { Ok((service.open(&id)?, service.action_outcomes(&id)?)) })
        .await;
    browser
        .update_in(cx, |this, window, cx| {
            // A command or selection change may have happened during the read.
            if !this.busy && this.selected_id().as_deref() == Some(&selected) {
                this.apply_observation(result, window, cx);
            }
        })
        .is_ok()
}
