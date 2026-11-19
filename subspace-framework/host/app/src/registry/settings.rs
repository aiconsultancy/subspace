//! Registration builders for board-owned Settings controls.

use std::sync::Arc;

use subspace_dashboard::settings::BoardFields;

use super::BoardRegistration;

impl BoardRegistration {
    /// Read current manifest metadata each time Settings opens.
    // qual:api
    pub fn with_field_source(
        mut self,
        source: impl Fn() -> Option<BoardFields> + Send + Sync + 'static,
    ) -> Self {
        self.fields = Some(Arc::new(source));
        self
    }
}
