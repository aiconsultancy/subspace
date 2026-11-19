use super::super::ChatPanel;

impl ChatPanel {
    /// Seed the existing debug turn fixture without starting or focusing a runtime.
    // qual:test_helper
    pub fn set_test_failure_fixture(
        &mut self,
        value: &str,
        cx: &gpui_kit::App,
    ) -> Result<(), String> {
        let fixture = super::fixture::parse(value)
            .ok_or_else(|| "Unknown chat failure fixture".to_owned())?;
        self.show_fixture(fixture);
        self.log_accepted_errors(cx);
        Ok(())
    }

    /// Seed the normal startup-error path without starting a runtime.
    // qual:test_helper
    pub fn set_test_startup_error(&mut self, message: impl Into<String>, cx: &gpui_kit::App) {
        self.connected(Err(message.into()));
        self.log_accepted_errors(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::setup;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn public_fixture_setters_use_the_existing_error_paths(cx: &mut TestAppContext) {
        let (chat, cx) = setup(cx);
        chat.update(cx, |chat, cx| {
            assert!(chat.set_test_failure_fixture("not-a-fixture", cx).is_err());
            chat.set_test_failure_fixture("stopped:details", cx)
                .unwrap();
            assert!(chat.handle.is_none());
            assert!(
                chat.tab()
                    .failure
                    .as_ref()
                    .is_some_and(|shown| shown.details_open)
            );
            chat.set_test_startup_error("Fixture runtime unavailable", cx);
            assert!(chat.handle.is_none());
            assert_eq!(chat.error.as_deref(), Some("Fixture runtime unavailable"));
        });
    }
}
