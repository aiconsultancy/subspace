//! The right-click menu a row, a group header or the detail panel offers.
//!
//! A dashboard says what its row is ([`RowMenu`]); which entries that earns
//! is a pure decision ([`row_menu_items`], [`panel_menu_items`]) the app and
//! the dashboards share, so every menu offers the same things in the same
//! order. [`popup`] turns the decision into GPUI Kit's `PopupMenu`, the menu
//! the session list already uses.

use gpui_kit::SharedString;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};

use crate::links::{self, safe_url};
use crate::{AddSelectionToChat, AddToChat, Callback, Row, set_selection};

/// Whether a menu offers to send what it shows to the chat. Menus offer it
/// unless their board keeps its content out of chat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MenuChat {
    #[default]
    Offered,
    /// Leaves out Add to chat and Add selection to chat.
    Off,
}

/// What a dashboard knows about the row the menu was opened over. Only the
/// facts a dashboard already has: nothing here makes a service call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowMenu {
    /// The row's title, for Copy title.
    pub title: SharedString,
    /// The short identifier the row shows, such as `1258950`.
    pub id: Option<SharedString>,
    /// The item's page in its service. Only `http` and `https` links are
    /// kept, so a menu never offers to open anything else.
    pub url: Option<SharedString>,
    /// The service the link belongs to, for "Open in GitHub".
    pub service: Option<SharedString>,
    /// Whether the row has details to open, which is Enter's action.
    pub opens_details: bool,
    /// Whether the menu offers the chat entries.
    pub chat: MenuChat,
}

impl RowMenu {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// The default for a list row: its title and id, with details to open.
    pub fn from_row(row: &Row) -> Self {
        Self {
            title: row.title.clone(),
            id: row.id.clone(),
            url: None,
            service: None,
            opens_details: true,
            chat: MenuChat::Offered,
        }
    }

    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The item's page in `service`. A link that is not `http` or `https` is
    /// dropped, so Open and Copy link never offer it.
    pub fn link(mut self, service: impl Into<SharedString>, url: &str) -> Self {
        self.url = browser_url(url).map(SharedString::from);
        self.service = Some(service.into());
        self
    }

    pub fn opens_details(mut self, opens: bool) -> Self {
        self.opens_details = opens;
        self
    }
}

/// A link a browser may open: `http` or `https` only.
fn browser_url(url: &str) -> Option<String> {
    safe_url(url).filter(|url| url.starts_with("http://") || url.starts_with("https://"))
}

/// One entry of a context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowMenuItem {
    /// The whole row and its open details, through the shell's `AddToChat`.
    AddToChat,
    /// The text selected in the detail panel.
    CopySelection,
    AddSelectionToChat,
    /// The row's Enter action.
    OpenDetails,
    OpenInBrowser {
        label: SharedString,
        url: SharedString,
    },
    CopyLink(SharedString),
    CopyTitle(SharedString),
    CopyId(SharedString),
    Separator,
}

/// What a row's menu offers, in order: the chat, then opening it, then the
/// copies. A row offers the chat unless its board turned it off and, when
/// it has one, its title.
pub fn row_menu_items(facts: &RowMenu) -> Vec<RowMenuItem> {
    let mut items = chat_items(facts.chat);
    let opening = opening_items(facts, facts.opens_details);
    let copies = copy_items(facts);
    extend_group(&mut items, opening);
    extend_group(&mut items, copies);
    items
}

/// What the detail panel's menu offers: the selection first when there is
/// one, then the item the panel shows. The panel is already open, so it
/// never offers to open its own details.
pub fn panel_menu_items(facts: &RowMenu, selection: &str) -> Vec<RowMenuItem> {
    let mut items = selection_items(selection, facts.chat);
    extend_group(&mut items, chat_items(facts.chat));
    extend_group(&mut items, opening_items(facts, false));
    extend_group(&mut items, copy_items(facts));
    items
}

/// Add to chat, when the menu offers the chat.
fn chat_items(chat: MenuChat) -> Vec<RowMenuItem> {
    match chat {
        MenuChat::Offered => vec![RowMenuItem::AddToChat],
        MenuChat::Off => Vec::new(),
    }
}

/// Copy and Add selection, for a selection that holds more than spaces.
fn selection_items(selection: &str, chat: MenuChat) -> Vec<RowMenuItem> {
    if selection.trim().is_empty() {
        return Vec::new();
    }
    match chat {
        MenuChat::Offered => vec![RowMenuItem::CopySelection, RowMenuItem::AddSelectionToChat],
        MenuChat::Off => vec![RowMenuItem::CopySelection],
    }
}

/// Open details and Open in the service's site, when either applies.
fn opening_items(facts: &RowMenu, details: bool) -> Vec<RowMenuItem> {
    let mut items = Vec::new();
    if details {
        items.push(RowMenuItem::OpenDetails);
    }
    if let Some(url) = &facts.url {
        let service = facts.service.clone().unwrap_or_else(|| "browser".into());
        items.push(RowMenuItem::OpenInBrowser {
            label: format!("Open in {service}").into(),
            url: url.clone(),
        });
    }
    items
}

/// Copy link, title and id, for whichever the row has.
fn copy_items(facts: &RowMenu) -> Vec<RowMenuItem> {
    let mut items = Vec::new();
    if let Some(url) = &facts.url {
        items.push(RowMenuItem::CopyLink(url.clone()));
    }
    if !facts.title.trim().is_empty() {
        items.push(RowMenuItem::CopyTitle(facts.title.clone()));
    }
    if let Some(id) = facts.id.clone().filter(|id| !id.trim().is_empty()) {
        items.push(RowMenuItem::CopyId(id));
    }
    items
}

/// Appends a group of entries, with a separator when both sides have some.
fn extend_group(items: &mut Vec<RowMenuItem>, group: Vec<RowMenuItem>) {
    if group.is_empty() {
        return;
    }
    if !items.is_empty() {
        items.push(RowMenuItem::Separator);
    }
    items.extend(group);
}

/// Builds `items` into `menu`. `open` is the row's Enter action, which only
/// a caller that has a row can give.
pub fn popup(mut menu: PopupMenu, items: Vec<RowMenuItem>, open: Option<Callback>) -> PopupMenu {
    for item in items {
        menu = add_item(menu, item, open.clone());
    }
    menu
}

fn add_item(menu: PopupMenu, item: RowMenuItem, open: Option<Callback>) -> PopupMenu {
    match item {
        // The action carries its own key hint, and the shell handles it.
        RowMenuItem::AddToChat => menu.menu("Add to chat", Box::new(AddToChat)),
        RowMenuItem::AddSelectionToChat => {
            menu.menu("Add selection to chat", Box::new(AddSelectionToChat))
        }
        RowMenuItem::CopySelection => menu.item(copy_item("Copy", String::new())),
        RowMenuItem::OpenDetails => menu.item(open_item(open)),
        RowMenuItem::OpenInBrowser { label, url } => menu.item(browser_item(label, url)),
        RowMenuItem::CopyLink(url) => menu.item(copy_item("Copy link", url.to_string())),
        RowMenuItem::CopyTitle(title) => menu.item(copy_item("Copy title", title.to_string())),
        RowMenuItem::CopyId(id) => menu.item(copy_item("Copy id", id.to_string())),
        RowMenuItem::Separator => menu.separator(),
    }
}

/// Copies `text`, or the window's selection when `text` is empty, which is
/// what the panel's Copy entry does.
fn copy_item(label: &'static str, text: String) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let text = match text.is_empty() {
            true => gpui_kit::base::TextSelection::selected_text(window, cx),
            false => text.clone(),
        };
        links::copy(text, cx);
    })
}

fn browser_item(label: SharedString, url: SharedString) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, _, cx| {
        links::open(&url, cx);
    })
}

/// Open details. Without a callback the entry is there but does nothing,
/// which cannot happen: the item is only offered when the caller gave one.
fn open_item(open: Option<Callback>) -> PopupMenuItem {
    PopupMenuItem::new("Open details").on_click(move |_, window, cx| {
        if let Some(open) = open.clone() {
            open(window, cx);
        }
    })
}

/// Holds the window's selection for the shell's `AddSelectionToChat`, which
/// runs after the menu dismisses and cannot read it then.
pub fn hold_selection(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) -> String {
    let text = gpui_kit::base::TextSelection::selected_text(window, cx);
    set_selection(cx, text.clone());
    text
}

#[cfg(test)]
mod tests {
    use super::{MenuChat, RowMenu, RowMenuItem, panel_menu_items, row_menu_items};
    use crate::list::{Glyph, Row};
    use crate::tokens::Tone;

    fn row() -> Row {
        Row {
            key: "octo/app#12".into(),
            icon: Glyph::new("●", Tone::Green),
            id: Some("#12".into()),
            title: "Port the board".into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        }
    }

    #[test]
    fn a_plain_row_offers_the_chat_its_details_and_its_text() {
        let items = row_menu_items(&RowMenu::from_row(&row()));
        assert_eq!(
            items,
            [
                RowMenuItem::AddToChat,
                RowMenuItem::Separator,
                RowMenuItem::OpenDetails,
                RowMenuItem::Separator,
                RowMenuItem::CopyTitle("Port the board".into()),
                RowMenuItem::CopyId("#12".into()),
            ]
        );
    }

    #[test]
    fn a_linked_row_offers_its_service_and_its_link() {
        let facts = RowMenu::from_row(&row()).link("GitHub", "https://github.com/octo/app/pull/12");
        let items = row_menu_items(&facts);
        assert!(items.contains(&RowMenuItem::OpenInBrowser {
            label: "Open in GitHub".into(),
            url: "https://github.com/octo/app/pull/12".into(),
        }));
        assert!(items.contains(&RowMenuItem::CopyLink(
            "https://github.com/octo/app/pull/12".into()
        )));
    }

    #[test]
    fn only_browser_links_are_offered() {
        let facts = RowMenu::new("Row").link("GitHub", "javascript:alert(1)");
        assert_eq!(facts.url, None);
        let items = row_menu_items(&facts);
        assert!(!items.iter().any(|item| matches!(
            item,
            RowMenuItem::OpenInBrowser { .. } | RowMenuItem::CopyLink(_)
        )));
        let mail = RowMenu::new("Row").link("Mail", "mailto:user@example.com");
        assert_eq!(mail.url, None, "a mail link is not a page to open");
    }

    #[test]
    fn a_row_without_details_or_text_still_offers_the_chat() {
        let facts = RowMenu::new("  ").opens_details(false);
        assert_eq!(row_menu_items(&facts), [RowMenuItem::AddToChat]);
    }

    #[test]
    fn a_group_header_row_offers_what_it_has() {
        let facts = RowMenu::new("work/repo")
            .id("12 risks")
            .opens_details(false);
        assert_eq!(
            row_menu_items(&facts),
            [
                RowMenuItem::AddToChat,
                RowMenuItem::Separator,
                RowMenuItem::CopyTitle("work/repo".into()),
                RowMenuItem::CopyId("12 risks".into()),
            ]
        );
    }

    #[test]
    fn the_panel_offers_the_selection_first_and_never_its_own_details() {
        let facts = RowMenu::from_row(&row()).link("GitHub", "https://github.com/octo/app/pull/12");
        let items = panel_menu_items(&facts, " select 1 ");
        assert_eq!(items[0], RowMenuItem::CopySelection);
        assert_eq!(items[1], RowMenuItem::AddSelectionToChat);
        assert!(items.contains(&RowMenuItem::AddToChat));
        assert!(!items.contains(&RowMenuItem::OpenDetails));
    }

    #[test]
    fn the_panel_without_a_selection_starts_at_the_item() {
        let items = panel_menu_items(&RowMenu::from_row(&row()), "   ");
        assert_eq!(items[0], RowMenuItem::AddToChat);
        assert!(!items.contains(&RowMenuItem::CopySelection));
    }

    #[test]
    fn menus_offer_the_chat_unless_the_board_turns_it_off() {
        assert_eq!(RowMenu::from_row(&row()).chat, MenuChat::Offered);
        assert_eq!(RowMenu::default().chat, MenuChat::Offered);
        let facts = RowMenu {
            chat: MenuChat::Off,
            ..RowMenu::from_row(&row())
        };
        assert_eq!(
            row_menu_items(&facts),
            [
                RowMenuItem::OpenDetails,
                RowMenuItem::Separator,
                RowMenuItem::CopyTitle("Port the board".into()),
                RowMenuItem::CopyId("#12".into()),
            ]
        );
        let bare = RowMenu {
            chat: MenuChat::Off,
            ..RowMenu::new("  ").opens_details(false)
        };
        assert_eq!(row_menu_items(&bare), []);
    }

    #[test]
    fn a_chat_free_panel_keeps_copy_and_leaves_out_both_chat_entries() {
        let facts = RowMenu {
            chat: MenuChat::Off,
            ..RowMenu::from_row(&row())
        };
        let items = panel_menu_items(&facts, "selected words");
        assert_eq!(
            items,
            [
                RowMenuItem::CopySelection,
                RowMenuItem::Separator,
                RowMenuItem::CopyTitle("Port the board".into()),
                RowMenuItem::CopyId("#12".into()),
            ]
        );
        assert_eq!(
            panel_menu_items(&facts, "  ")[0],
            RowMenuItem::CopyTitle("Port the board".into())
        );
    }
}
