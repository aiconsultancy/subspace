//! The dialog title's breadcrumb, named with the navigation labels.

use gpui_kit::SharedString;
use subspace_dashboard::settings::SettingsSection;

use super::SettingsPage;
use super::model::app_items;

/// The breadcrumb of a destination: `Settings`, its group, then the page
/// and any board section names, using the navigation labels.
pub(in crate::settings) fn page_trail(
    page: &SettingsPage,
    boards: &[crate::registry::BoardRegistration],
    extensions: &[crate::SettingsExtension],
) -> Vec<SharedString> {
    let board = |id: &str| boards.iter().find(|board| board.id == id);
    let mut trail = vec![SharedString::from("Settings")];
    match page {
        SettingsPage::Board(id) => {
            trail.push("Space".into());
            trail.extend(board(id).map(|board| SharedString::from(board.title.to_string())));
        }
        SettingsPage::BoardSection { board: id, path } => {
            trail.push("Space".into());
            let board = board(id);
            trail.extend(board.map(|board| SharedString::from(board.title.to_string())));
            let sections = board.map_or(&[][..], |board| board.settings.sections);
            trail.extend(section_titles(sections, path));
        }
        SettingsPage::Extension(id) => {
            trail.push("Application".into());
            trail.extend(
                extensions
                    .iter()
                    .find(|extension| extension.id == *id)
                    .map(|extension| SharedString::from(extension.title)),
            );
        }
        page => {
            trail.push("Application".into());
            trail.extend(
                app_items(true)
                    .into_iter()
                    .chain([super::model::TERMINAL_ITEM])
                    .find(|(_, _, destination, _)| destination == page)
                    .map(|(_, label, _, _)| SharedString::from(label)),
            );
        }
    }
    trail
}

/// The titles along a registration path of nested board sections.
fn section_titles(mut sections: &'static [SettingsSection], path: &[usize]) -> Vec<SharedString> {
    let mut titles = Vec::new();
    for index in path {
        let Some(section) = sections.get(*index) else {
            break;
        };
        titles.push(SharedString::from(section.title));
        sections = section.children;
    }
    titles
}
