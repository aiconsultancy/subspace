use std::path::Path;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, ElementId, TestAppContext, Window, WindowHandle, px, size};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;

use crate::settings::{Settings, SettingsPage};

pub(super) fn open(root: &Path, saved: bool, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        let boards = ["actions", "github-copilot-agents", "terminal"].map(|id| {
            let mut board = crate::settings::navigation_tests::registration();
            board.id = id;
            board.title = id.into();
            board.settings.sections = &[];
            board.connection = connection(id);
            board
        });
        cx.set_global(crate::registry::IncludedBoards(boards.into()));
    });
    let path = root.join("config.json");
    let mut source = serde_json::json!({
        "github": {},
        "repos": ["owner/repo"]
    });
    if saved {
        source["github"]["token"] = "test-only-nonworking-saved-credential".into();
    }
    std::fs::write(&path, source.to_string()).unwrap();
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    cx.simulate_window_resize(settings.into(), size(px(1100.), px(1800.)));
    settings
}

fn connection(id: &str) -> subspace_dashboard::settings::ConnectionSettings {
    use subspace_dashboard::settings::{ConnectionField as Field, ConnectionSettings};
    let fields: &'static [Field] = match id {
        "actions" => &[Field::GitHubCredentials, Field::Repositories],
        "github-copilot-agents" => &[
            Field::GitHubCredentials,
            Field::Repositories,
            Field::Note(
                "Configure task filters, alerts and agent behavior in the Copilot Agents dashboard.",
            ),
        ],
        _ => &[Field::Note(
            "Choose shells and their placement in the Terminal dashboard.",
        )],
    };
    ConnectionSettings::new(fields)
}

pub(super) fn rendered<T>(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window, &mut App) -> T,
) -> T {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        check(window, cx)
    })
    .unwrap()
}

pub(super) fn show(settings: WindowHandle<Settings>, board: &'static str, cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, _, cx| {
            settings.page = SettingsPage::Board(board);
            cx.notify();
        })
        .unwrap();
}

pub(super) fn control_ids(
    settings: WindowHandle<Settings>,
    board: &str,
    cx: &mut TestAppContext,
) -> Vec<ElementId> {
    settings
        .read_with(cx, |_, _| {
            let repositories = "settings-repositories-list".into();
            match board {
                "actions" | "github-copilot-agents" => vec![
                    "settings-github-account".into(),
                    "secret-input".into(),
                    repositories,
                ],
                _ => Vec::new(),
            }
        })
        .unwrap()
}
