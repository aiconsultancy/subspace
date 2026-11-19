//! Shared terminal close policy from Ghostty configuration and foreground program.
use subspace_ghostty_config::ConfirmCloseSurface;

pub fn should_confirm(
    policy: Option<ConfirmCloseSurface>,
    busy: Option<&str>,
    shell: &str,
) -> bool {
    match policy {
        Some(ConfirmCloseSurface::Never) => false,
        Some(ConfirmCloseSurface::Always) => true,
        None => busy.is_some_and(|program| program != shell),
    }
}

#[cfg(test)]
pub fn preference_contains_session_content(key: &str, value: &str) -> bool {
    key.starts_with("terminal.") && !value.contains("\n") && !value.contains("\r")
}

#[cfg(test)]
mod tests {
    use super::{preference_contains_session_content, should_confirm};
    use subspace_ghostty_config::ConfirmCloseSurface;

    #[test]
    fn close_confirmation_follows_surface_policy_and_busy_program() {
        assert!(!should_confirm(None, None, "zsh"));
        assert!(!should_confirm(None, Some("zsh"), "zsh"));
        assert!(should_confirm(None, Some("sleep"), "zsh"));
        assert!(!should_confirm(
            Some(ConfirmCloseSurface::Never),
            Some("sleep"),
            "zsh"
        ));
        assert!(should_confirm(
            Some(ConfirmCloseSurface::Always),
            None,
            "zsh"
        ));
    }

    #[test]
    fn preference_guard_rejects_multiline_session_content() {
        assert!(preference_contains_session_content(
            "terminal.confirm",
            "true"
        ));
        assert!(!preference_contains_session_content(
            "terminal.screen",
            "hello\nworld"
        ));
    }
}
