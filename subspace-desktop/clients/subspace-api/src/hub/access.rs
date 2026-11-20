//! One automatic retry for a native session that becomes usable after startup.

use gpui_kit::App;

use super::{Hub, HubStatus, ReadError, ReadPriority, Task, lock};
use crate::{NativeLease, native};

impl Hub {
    /// Resume the initial connection once this hub's native account is usable.
    pub fn reconnect_when_account_ready(&self, cx: &App) {
        if let Some(NativeLease(lease)) = self.0.client.native_lease {
            self.retry_initial_access(|| native::ready(cx, lease));
        }
    }

    pub(crate) fn retry_initial_access(&self, ready: impl FnOnce() -> bool) {
        let mut state = lock(&self.0.state);
        if !state.initial_access_retry
            || state.snapshot.list_loading
            || state.snapshot.status != HubStatus::Error(ReadError::SignInNeeded)
            || !ready()
        {
            return;
        }
        state.initial_access_retry = false;
        state.snapshot.status = HubStatus::Loading;
        state.changed();
        self.0
            .enqueue(&mut state, Task::Bootstrap, ReadPriority::Visible);
    }
}
