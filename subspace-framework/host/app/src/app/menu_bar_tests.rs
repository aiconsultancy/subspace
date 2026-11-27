use gpui_kit::{Menu, MenuItem};

use super::{Layout, MenuBar, menus};

/// Each menu as its title and its items: `-` for a separator, `name >` for a
/// submenu and `name *` for a menu the system fills.
fn outline(menus: &[Menu]) -> Vec<(String, Vec<String>)> {
    menus
        .iter()
        .map(|menu| (menu.name.to_string(), items(&menu.items)))
        .collect()
}

fn items(items: &[MenuItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Separator => "-".to_owned(),
            MenuItem::Submenu(menu) => format!("{} >", menu.name),
            MenuItem::SystemMenu(menu) => format!("{} *", menu.name),
            MenuItem::Action { name, .. } => name.to_string(),
        })
        .collect()
}

pub(super) fn bar(terminal: bool, report_issue: bool) -> MenuBar {
    MenuBar {
        name: "Example".to_owned(),
        terminal,
        report_issue,
        developer: None,
    }
}

fn menu(menus: &[(String, Vec<String>)], name: &str) -> Vec<String> {
    menus
        .iter()
        .find(|(title, _)| title == name)
        .map(|(_, items)| items.clone())
        .unwrap_or_default()
}

fn titles(menus: &[(String, Vec<String>)]) -> Vec<&str> {
    menus.iter().map(|(title, _)| title.as_str()).collect()
}

const EDIT: [&str; 7] = ["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All"];

const VIEW: [&str; 15] = [
    "Command Palette",
    "Search Items",
    "Search Content",
    "-",
    "Chat",
    "Navigator",
    "Inspector",
    "Vertical Space Navigation",
    "-",
    "Next Space",
    "Previous Space",
    "-",
    "Wrap Titles",
    "Theme Designer",
    "Reload",
];

#[test]
fn the_mac_menu_bar_follows_apples_layout() {
    let bar = outline(&menus(bar(false, false), Layout::Mac));
    assert_eq!(titles(&bar), ["Example", "Edit", "View", "Window", "Help"]);
    assert_eq!(
        menu(&bar, "Example"),
        [
            "About Example",
            "-",
            "Settings…",
            "-",
            "Services *",
            "-",
            "Hide Example",
            "Hide Others",
            "Show All",
            "-",
            "Quit Example",
        ]
    );
    assert_eq!(menu(&bar, "Edit"), EDIT);
    assert_eq!(menu(&bar, "View"), VIEW);
    assert_eq!(menu(&bar, "Window"), ["Minimize", "Zoom"]);
    assert_eq!(
        menu(&bar, "Help"),
        ["Example Help", "Keyboard Shortcuts", "-", "Show Logs"]
    );
}

#[test]
fn windows_and_linux_keep_settings_in_file_and_about_in_help() {
    for (layout, quit) in [(Layout::Windows, "Exit"), (Layout::Linux, "Quit")] {
        let bar = outline(&menus(bar(false, false), layout));
        assert_eq!(titles(&bar), ["File", "Edit", "View", "Help"]);
        assert_eq!(menu(&bar, "File"), ["Settings…", "-", quit]);
        assert_eq!(menu(&bar, "Edit"), EDIT);
        assert_eq!(menu(&bar, "View")[..VIEW.len()], VIEW);
        assert_eq!(menu(&bar, "View")[VIEW.len()..], ["-", "Full Screen"]);
        assert_eq!(
            menu(&bar, "Help"),
            [
                "Example Help",
                "Keyboard Shortcuts",
                "-",
                "Show Logs",
                "-",
                "About Example",
            ]
        );
    }
}

#[test]
fn the_view_menu_lists_the_terminal_only_for_a_product_with_one() {
    for layout in [Layout::Mac, Layout::Windows, Layout::Linux] {
        let without = menu(&outline(&menus(bar(false, false), layout)), "View");
        assert!(!without.iter().any(|item| item.contains("erminal")));
        let with = menu(&outline(&menus(bar(true, false), layout)), "View");
        let chat = with.iter().position(|item| item == "Chat");
        assert_eq!(
            with[chat.map_or(0, |at| at + 1)..][..2],
            ["Terminal", "Terminal Layout >"]
        );
    }
    let view = menus(bar(true, false), Layout::current())
        .into_iter()
        .find(|menu| menu.name == "View");
    let layout = view
        .iter()
        .flat_map(|menu| &menu.items)
        .find_map(|item| match item {
            MenuItem::Submenu(menu) => Some(items(&menu.items)),
            _ => None,
        });
    assert_eq!(
        layout.unwrap_or_default(),
        [
            "Move Left",
            "Move to Bottom",
            "Move Right",
            "Full Window Width",
            "Maximize",
        ]
    );
}

#[test]
fn the_developer_menu_follows_view_and_help_comes_last() {
    for layout in [Layout::Mac, Layout::Windows, Layout::Linux] {
        let mut with = bar(false, false);
        with.developer = Some(Menu::new("Developer"));
        let bar = outline(&menus(with, layout));
        let titles = titles(&bar);
        let view = titles.iter().position(|title| *title == "View");
        assert_eq!(titles.get(view.map_or(0, |at| at + 1)), Some(&"Developer"));
        assert_eq!(titles.last(), Some(&"Help"));
    }
}

#[cfg(feature = "report-issue")]
#[test]
fn help_offers_report_an_issue_only_when_the_app_names_a_repository() {
    let help =
        |report_issue, layout| menu(&outline(&menus(bar(false, report_issue), layout)), "Help");
    assert!(!help(false, Layout::Mac).contains(&"Report an Issue…".to_owned()));
    assert_eq!(
        help(true, Layout::Mac).last().map(String::as_str),
        Some("Report an Issue…")
    );
    assert_eq!(
        help(true, Layout::Linux),
        [
            "Example Help",
            "Keyboard Shortcuts",
            "-",
            "Show Logs",
            "Report an Issue…",
            "-",
            "About Example",
        ]
    );
}

#[cfg(not(feature = "report-issue"))]
#[test]
fn help_has_no_report_an_issue_without_the_feature() {
    let help = menu(
        &outline(&menus(bar(false, true), Layout::current())),
        "Help",
    );
    assert!(!help.iter().any(|item| item.contains("Issue")));
}
