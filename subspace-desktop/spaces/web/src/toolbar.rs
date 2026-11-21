//! Page controls in the Space's pane toolbar, above the page: where the page
//! goes, then the Space's title and where the page is, on the leading side,
//! and the page's other action on the trailing side.

use gpui_kit::component::Disableable as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::toolbar_label::{self, ToolbarLabel};
use subspace_framework::dashboard::workspace::Toolbar;
use subspace_framework::dashboard::{Tokens, heading_font};
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

/// What a control does, its element id, its glyph and its name.
type Control = (Navigation, &'static str, SvgIcon, &'static str);

/// Where the page goes: the toolbar's leading controls, in order.
const NAVIGATION: [Control; 4] = [
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
];

/// The page's other action, on the toolbar's trailing side.
const OPEN_IN_BROWSER: Control = (
    Navigation::Browser,
    "web-browser",
    IconName::ExternalLink,
    "Open in browser",
);

/// The Space's pane toolbar. Its controls are disabled until the page
/// exists. A local app has no Open in browser, which its private address
/// cannot use.
pub(crate) fn render(board: &WebBoard, cx: &mut Context<WebBoard>) -> Toolbar {
    let enabled = board.page.is_some();
    let toolbar = NAVIGATION
        .into_iter()
        .fold(Toolbar::new("web-toolbar"), |toolbar, item| {
            toolbar.leading(control(item, enabled, cx))
        });
    let toolbar = toolbar.leading(title(board.config.title.clone().into(), cx));
    let toolbar = match place(board.state.url.as_deref(), &board.start.url) {
        Some(site) => toolbar.leading(place_label(site, cx)),
        None => toolbar,
    };
    match board.start.app.is_some() {
        true => toolbar,
        false => toolbar.trailing(browser_control(enabled, cx)),
    }
}

/// Open in browser, with its name beside its glyph.
fn browser_control(enabled: bool, cx: &mut Context<WebBoard>) -> IconButton {
    let (.., name) = OPEN_IN_BROWSER;
    control(OPEN_IN_BROWSER, enabled, cx).label(name)
}

fn control(item: Control, enabled: bool, cx: &mut Context<WebBoard>) -> IconButton {
    let (navigation, id, icon, name) = item;
    icon_button(id, Intent::Quiet, icon, name)
        .disabled(!enabled)
        .on_click(cx.listener(move |board, _: &ClickEvent, _, cx| {
            board.navigate(navigation, cx);
        }))
}

/// The Space's own name. A web Space has no navigator to carry it, so it
/// sits in the toolbar.
fn title(text: SharedString, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    div()
        .id("web-title")
        .role(Role::Heading)
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .pl_1()
        .truncate()
        .font_family(heading_font(cx))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(tokens.row_font)
        .text_color(tokens.fg_strong)
        .child(text)
        .into_any_element()
}

/// Where the page is, for the quiet line beside the title: the page's own
/// address once it has reported one, and the start address until then.
fn place(current: Option<&str>, start: &str) -> Option<String> {
    policy::location(current.unwrap_or(start))
}

/// Where the page is as a toolbar fact: it names the real site and promises
/// no action.
fn place_label(site: String, cx: &App) -> AnyElement {
    let label = ToolbarLabel {
        icon: Some(IconName::Globe.into()),
        accessible: policy::spoken_location(&site).into(),
        text: site.into(),
    };
    toolbar_label::label("web-place", label, cx).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{NAVIGATION, Navigation, OPEN_IN_BROWSER, place};

    #[test]
    fn navigation_leads_in_order_and_open_in_browser_trails() {
        assert_eq!(
            NAVIGATION.map(|(navigation, ..)| navigation),
            [
                Navigation::Back,
                Navigation::Forward,
                Navigation::Reload,
                Navigation::Home,
            ]
        );
        assert_eq!(
            NAVIGATION.map(|(_, id, ..)| id),
            ["web-back", "web-forward", "web-reload", "web-home"]
        );
        let (navigation, id, _, name) = OPEN_IN_BROWSER;
        assert_eq!(
            (navigation, id, name),
            (Navigation::Browser, "web-browser", "Open in browser")
        );
    }

    #[test]
    fn the_place_is_the_start_address_until_the_page_reports_its_own() {
        let start = "https://wiki.example.com/start";
        assert_eq!(place(None, start).as_deref(), Some("wiki.example.com"));
        assert_eq!(
            place(Some("https://docs.example.com/a"), start).as_deref(),
            Some("docs.example.com")
        );
        assert_eq!(
            place(None, "subspace://notes/").as_deref(),
            Some("Local web app")
        );
    }

    #[test]
    fn a_page_without_a_site_has_no_place() {
        let start = "https://wiki.example.com/";
        assert_eq!(place(Some("about:blank"), start), None);
        assert_eq!(place(None, ""), None);
    }
}
