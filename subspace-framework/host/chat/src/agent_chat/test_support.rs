//! Offline handle installation for host-owned integration tests.
pub use subspace_agent::{AgentHandle, ChatCommand, Command};

impl super::ChatPanel {
    // qual:test_helper
    pub fn set_test_runtime(&mut self, handle: AgentHandle) {
        self.engine = handle.engine();
        self.connected(Ok(handle));
    }
}
