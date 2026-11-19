//! The single constructor boundary for native Markdown parsing.
use gpui_kit::base::TextViewState;
use gpui_kit::{App, AppContext as _, Entity};
/// The constructor is a defensive boundary even when a caller has prepared text.
/// Rechecking uses the shared preflight cache and deliberately keeps raw input safe.
pub(crate) fn create(source: &str, cx: &mut App) -> Entity<TextViewState> {
    let source = super::markdown_guard::prepare(source.to_owned().into());
    cx.new(|cx| TextViewState::markdown(&source, cx))
}
