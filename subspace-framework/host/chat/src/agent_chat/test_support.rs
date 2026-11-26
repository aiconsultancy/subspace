//! Offline handle installation for host-owned integration tests.
pub use subspace_agent::{AgentHandle, ChatCommand, Command};

impl super::ChatPanel {
    // qual:test_helper
    pub fn set_test_runtime(&mut self, handle: AgentHandle) {
        self.engine = handle.engine();
        self.connected(Ok(handle));
    }

    /// Shows the Sessions tab, as its header button does.
    // qual:test_helper
    pub fn show_test_sessions(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.show_sessions(window, cx);
    }
}
