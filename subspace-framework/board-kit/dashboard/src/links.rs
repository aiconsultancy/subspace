//! Open in browser and copy (REQ-15). Only `http`, `https` and `mailto`
//! links leave the app.

use gpui_kit::base::Link;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClipboardItem, ElementId, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, Styled as _,
};

use crate::anchored::AnchoredContextMenuExt as _;

/// A themed external resource link with native keyboard activation and a
/// context menu that copies its destination without opening it.
pub fn external(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    url: impl Into<SharedString>,
    tokens: &crate::Tokens,
) -> AnyElement {
    external_link(id.into(), label.into(), url.into(), tokens, false)
}

/// An [`external`] link with a trailing decorative external-link glyph, for
/// inline values where the destination leaving the app is not obvious.
pub fn external_marked(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    url: impl Into<SharedString>,
    tokens: &crate::Tokens,
) -> AnyElement {
    external_link(id.into(), label.into(), url.into(), tokens, true)
}

fn external_link(
    id: ElementId,
    label: SharedString,
    url: SharedString,
    tokens: &crate::Tokens,
    marked: bool,
) -> AnyElement {
    let accent = tokens.accent;
    let hover_foreground = tokens.fg_strong;
    let focus_background = tokens.select;
    Link::new(id)
        .href(url.clone())
        .accessibility_label(label.clone())
        .text_color(accent)
        .text_decoration_1()
        .rounded_sm()
        .cursor_pointer()
        .hover(move |style| style.text_color(hover_foreground))
        .focus_visible(move |style| style.bg(focus_background))
        .open_with(|url, _, _, cx| {
            open(url, cx);
        })
        .child(label)
        .when(marked, |link| {
            link.flex()
                .min_w_0()
                .items_center()
                .gap_1()
                .child(Icon::from(subspace_lucide::IconName::ExternalLink).xsmall())
        })
        .anchored_context_menu(move |menu, _, _| {
            let url = url.clone();
            menu.item(
                PopupMenuItem::new("Copy link")
                    .icon(subspace_lucide::IconName::Copy)
                    .on_click(move |_, _, cx| copy(url.to_string(), cx)),
            )
        })
        .into_any_element()
}

/// The URL when it is absolute and `http`, `https` or `mailto`; else `None`.
pub fn safe_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw.trim()).ok()?;
    matches!(url.scheme(), "http" | "https" | "mailto").then(|| url.to_string())
}

/// Opens `url` in the system browser when [`safe_url`] allows it. Returns
/// whether it did.
pub fn open(url: &str, cx: &App) -> bool {
    match safe_url(url) {
        Some(url) => {
            cx.open_url(&url);
            true
        }
        None => false,
    }
}

/// Puts `text` on the system clipboard.
pub fn copy(text: impl Into<String>, cx: &App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
}

/// The program that shows a folder on this platform.
fn folder_opener() -> &'static str {
    match std::env::consts::OS {
        "macos" => "open",
        "windows" => "explorer",
        _ => "xdg-open",
    }
}

/// Shows `directory` in the platform's file manager, without waiting. An
/// opener that cannot start is reported.
pub fn reveal_directory(directory: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(folder_opener())
        .arg(directory)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Cannot open {}: {error}", directory.display()))
}

#[cfg(test)]
mod interaction_tests;

#[cfg(test)]
mod tests {
    use super::{folder_opener, safe_url};

    #[test]
    fn the_folder_opener_is_a_known_program() {
        assert!(["open", "explorer", "xdg-open"].contains(&folder_opener()));
    }

    #[test]
    fn only_web_and_mail_links_pass() {
        assert_eq!(
            safe_url(" https://dev.azure.com/x?a=1 ").as_deref(),
            Some("https://dev.azure.com/x?a=1")
        );
        assert!(safe_url("http://example.com").is_some());
        assert!(safe_url("mailto:user@example.com").is_some());
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>",
            "file:///etc/passwd",
            "/relative/path",
            "vbscript:x",
            "",
        ] {
            assert_eq!(safe_url(bad), None, "{bad}");
        }
    }
}
