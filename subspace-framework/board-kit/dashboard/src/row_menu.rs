//! The right-click menu: every context menu is built from the same seven
//! groups in the same order ([`MenuGroup`]), with a separator between groups
//! that have commands. A board fills groups in with [`MenuCommand`]s; it
//! never orders them, and a group with nothing in it is left out.
//! [`build`] turns a [`GroupedMenu`] into GPUI Kit's `PopupMenu`.
//!
//! A list row, a group header or the detail panel says what it is
//! ([`RowMenu`]); which standard commands that earns is a pure decision
//! ([`row_menu_items`], [`panel_menu_items`]) the app and the boards share,
//! and [`grouped`] turns it into commands.

mod groups;
mod render;

pub use groups::{GroupedMenu, MenuCommand, MenuGroup, MenuLine};
pub use render::build;

use gpui_kit::SharedString;
use subspace_lucide::IconName;

use crate::links::{self, safe_url};
use crate::{Activate, AddSelectionToChat, AddToChat, Callback, Row, set_selection};

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

/// One of the standard commands a row, a group header or the detail panel
/// offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowMenuItem {
    /// The row's Enter action, "Open".
    OpenDetails,
    OpenInBrowser {
        label: SharedString,
        url: SharedString,
    },
    /// The text selected in the detail panel.
    CopySelection,
    CopyLink(SharedString),
    CopyTitle(SharedString),
    CopyId(SharedString),
    /// The whole row and its open details, through the shell's `AddToChat`.
    AddToChat,
    AddSelectionToChat,
}

impl RowMenuItem {
    /// The group the command belongs to.
    pub fn group(&self) -> MenuGroup {
        match self {
            Self::OpenDetails | Self::OpenInBrowser { .. } => MenuGroup::Open,
            Self::CopySelection | Self::CopyLink(_) | Self::CopyTitle(_) | Self::CopyId(_) => {
                MenuGroup::Copy
            }
            Self::AddToChat | Self::AddSelectionToChat => MenuGroup::Chat,
        }
    }
}

/// What a row's menu offers, in menu order: opening it, the copies, then
/// the chat. A row offers the chat unless its board turned it off and, when
/// it has one, its title.
pub fn row_menu_items(facts: &RowMenu) -> Vec<RowMenuItem> {
    let mut items = opening_items(facts, facts.opens_details);
    items.extend(copy_items(facts));
    items.extend(chat_items(facts.chat));
    items
}

/// What the detail panel's menu offers, in menu order: the service's page,
/// Copy for the selection with the item's copies, then the chat. The panel
/// is already open, so it never offers to open its own details.
pub fn panel_menu_items(facts: &RowMenu, selection: &str) -> Vec<RowMenuItem> {
    let selected = !selection.trim().is_empty();
    let mut items = opening_items(facts, false);
    if selected {
        items.push(RowMenuItem::CopySelection);
    }
    items.extend(copy_items(facts));
    items.extend(chat_items(facts.chat));
    if selected && facts.chat == MenuChat::Offered {
        items.push(RowMenuItem::AddSelectionToChat);
    }
    items
}

/// Add to chat, when the menu offers the chat.
fn chat_items(chat: MenuChat) -> Vec<RowMenuItem> {
    match chat {
        MenuChat::Offered => vec![RowMenuItem::AddToChat],
        MenuChat::Off => Vec::new(),
    }
}

/// Open and Open in the service's site, when either applies.
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

/// The commands `items` stand for, in their groups. `open` is the row's
/// Enter action, which only a caller that has a row can give; without it
/// the menu has no Open.
pub fn grouped(items: Vec<RowMenuItem>, open: Option<Callback>) -> GroupedMenu {
    let mut menu = GroupedMenu::new();
    menu.extend(
        items
            .into_iter()
            .filter_map(|item| command(item, open.clone())),
    );
    menu
}

/// One standard command, with its glyph.
fn command(item: RowMenuItem, open: Option<Callback>) -> Option<MenuCommand> {
    let group = item.group();
    Some(match item {
        RowMenuItem::OpenDetails => {
            let open = open?;
            MenuCommand::new(group, "Open", IconName::ArrowRight, move |window, cx| {
                open(window, cx)
            })
            .key(Activate)
        }
        RowMenuItem::OpenInBrowser { label, url } => {
            MenuCommand::new(group, label, IconName::ArrowUpRight, move |_, cx| {
                links::open(&url, cx);
            })
        }
        // An empty text copies the window's selection.
        RowMenuItem::CopySelection => copy_command("Copy", IconName::Copy, String::new()),
        RowMenuItem::CopyLink(url) => copy_command("Copy link", IconName::Link, url.to_string()),
        RowMenuItem::CopyTitle(title) => {
            copy_command("Copy title", IconName::Copy, title.to_string())
        }
        RowMenuItem::CopyId(id) => copy_command("Copy id", IconName::Copy, id.to_string()),
        // The action carries its own key hint, and the shell handles it.
        RowMenuItem::AddToChat => {
            MenuCommand::action(group, "Add to chat", IconName::MessageSquarePlus, AddToChat)
        }
        RowMenuItem::AddSelectionToChat => MenuCommand::action(
            group,
            "Add selection to chat",
            IconName::MessageSquareQuote,
            AddSelectionToChat,
        ),
    })
}

/// Copies `text`, or the window's selection when `text` is empty, which is
/// what the panel's Copy does.
fn copy_command(label: &'static str, icon: IconName, text: String) -> MenuCommand {
    MenuCommand::new(MenuGroup::Copy, label, icon, move |window, cx| {
        let text = match text.is_empty() {
            true => gpui_kit::base::TextSelection::selected_text(window, cx),
            false => text.clone(),
        };
        links::copy(text, cx);
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
mod tests;
