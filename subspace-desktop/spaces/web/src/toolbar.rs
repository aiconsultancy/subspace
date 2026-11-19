//! Page controls in the shell's board toolbar.

use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, IntoElement as _, ParentElement as _, Styled as _,
};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::WebBoard;
use crate::{page::Page, policy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Navigation {
    Back,
    Forward,
    Reload,
    Home,
    Browser,
}

pub(crate) fn navigate(
    page: &Page,
    start: &str,
    navigation: Navigation,
    cx: &mut App,
) -> Result<(), String> {
    match navigation {
        Navigation::Back => page.back(cx),
        Navigation::Forward => page.forward(cx),
        Navigation::Reload => page.reload(cx),
        Navigation::Home => page.load(start, cx),
        Navigation::Browser => {
            if let Some(url) = page.url(cx).and_then(|url| policy::browser_url(&url)) {
                cx.open_url(&url);
            }
            Ok(())
        }
    }
}

const CONTROLS: [(Navigation, &str, SvgIcon, &str); 5] = [
    (Navigation::Back, "web-back", IconName::ArrowLeft, "Back"),
    (
        Navigation::Forward,
        "web-forward",
        IconName::ArrowRight,
        "Forward",
    ),
    (
        Navigation::Reload,
        "web-reload",
        IconName::RotateCw,
        "Reload page",
    ),
    (
        Navigation::Home,
        "web-home",
        IconName::House,
        "Go to the start page",
    ),
    (
        Navigation::Browser,
        "web-browser",
        IconName::ExternalLink,
        "Open in browser",
    ),
];

/// The page controls. `browser` adds Open in browser, which a local app's
/// private address cannot use.
pub(crate) fn render(enabled: bool, browser: bool, cx: &mut Context<WebBoard>) -> AnyElement {
    h_flex()
        .gap_1()
        .children(
            CONTROLS
                .into_iter()
                .filter(|(navigation, ..)| browser || *navigation != Navigation::Browser)
                .map(|(navigation, id, icon, name)| {
                    icon_button(id, Intent::Quiet, icon, name)
                        .disabled(!enabled)
                        .on_click(cx.listener(move |board, _: &ClickEvent, _, cx| {
                            board.navigate(navigation, cx);
                        }))
                }),
        )
        .into_any_element()
}
