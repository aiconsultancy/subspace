//! Shell surfaces: the parts of the window that the host composes from
//! shared components. Boards cannot depend on the host, so each card links
//! to the cards of the components the surface is built from and, where the
//! shell has a command for it, opens the real surface.

use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{App, Context, ParentElement as _, Styled as _, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, action_button};
use subspace_lucide::IconName;

use super::{Sample, sample};
use crate::ComponentGallery;
use crate::catalog::Item;
use crate::view::{named, reveal_button};

const LINKS_WIDTH: f32 = 640.;

mod surfaces;

pub(crate) use surfaces::{SURFACES, Surface};

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "shell::window",
        source: "host/app/src/app/shell.rs",
        used_in: "The window frame: the board, the optional chat pane and Help dock in resizable splits, the status bar and overlays.",
        samples: window,
        kit: None,
    },
    Item {
        name: "shell::toolbar",
        source: "host/app/src/app/toolbar.rs",
        used_in: "The title bar: product mark, board tabs as glass capsules, the board's own toolbar and the global actions, each overflowing behind a chevron.",
        samples: toolbar,
        kit: None,
    },
    Item {
        name: "shell::navigation_rail",
        source: "host/app/src/app/navigation.rs",
        used_in: "The optional vertical board navigation, collapsible to icons.",
        samples: navigation_rail,
        kit: None,
    },
    Item {
        name: "shell::status_bar",
        source: "host/app/src/status_bar.rs",
        used_in: "Board status, the chat engine's state and the always-flat About, Report issue, Commands and Help buttons.",
        samples: status_bar,
        kit: None,
    },
    Item {
        name: "shell::command_palette",
        source: "host/app/src/app/palette_view.rs",
        used_in: "Shell and board commands, and the / item search.",
        samples: command_palette,
        kit: None,
    },
    Item {
        name: "shell::help",
        source: "host/app/src/app/help.rs",
        used_in: "Global keys, board guides, the shortcut list and app-wide About help.",
        samples: help,
        kit: None,
    },
    Item {
        name: "shell::settings_dialog",
        source: "host/app/src/app/settings_dialog.rs",
        used_in: "The Settings tree, pages and forms, with Save and Cancel.",
        samples: settings_dialog,
        kit: None,
    },
    Item {
        name: "shell::about_dialog",
        source: "host/app/src/app/about_dialog.rs",
        used_in: "Theme-aware product logo, name, version and channel; updates, repository, diagnostics and verbatim product credits. Open the real dialog to inspect every state.",
        samples: about_dialog,
        kit: None,
    },
    Item {
        name: "shell::report_issue",
        source: "host/report-issue/src/lib.rs",
        used_in: "The Report issue form, its diagnostics and image previews.",
        samples: report_issue,
        kit: None,
    },
    Item {
        name: "shell::theme_designer",
        source: "host/app/src/jig/view.rs",
        used_in: "Live theme, typography, density and colour editing; every edit previews in the whole window, this gallery included.",
        samples: theme_designer,
        kit: None,
    },
    Item {
        name: "shell::toasts",
        source: "host/app/src/app/toasts.rs",
        used_in: "The stack that shows board Toasts.",
        samples: toasts,
        kit: None,
    },
    Item {
        name: "shell::chat_panel",
        source: "host/chat/src/lib.rs",
        used_in: "The agent chat pane: tabs, transcript, approvals, sessions, input and attachments.",
        samples: chat_panel,
        kit: None,
    },
    Item {
        name: "shell::chat_canvas",
        source: "host/chat/src/agent_chat/canvas.rs",
        used_in: "Copilot's chart, diagram and drawing area beside a chat.",
        samples: chat_canvas,
        kit: None,
    },
    Item {
        name: "shell::search_view",
        source: "host/search-ui/src/search_view.rs",
        used_in: "Full-text search across boards, shown in place of the board.",
        samples: search_view,
        kit: None,
    },
    Item {
        name: "shell::log_view",
        source: "board-kit/dashboard/src/log_view.rs",
        used_in: "The app log, shown in place of the board. It shows real records, so the gallery opens it rather than drawing a copy.",
        samples: log_view,
        kit: None,
    },
];

/// The surface called `name`.
pub(crate) fn surface(name: &str) -> Option<&'static Surface> {
    SURFACES.iter().find(|surface| surface.name == name)
}

/// Whether the running app registers the shell action `name`.
fn available(name: &str, cx: &App) -> bool {
    cx.build_action(name, None).is_ok()
}

fn open_button(surface: &'static Surface, cx: &App) -> Option<Sample> {
    let action = surface.action?;
    let button = action_button(
        named("gallery-open", surface.name),
        Intent::Neutral,
        Some(IconName::SquareArrowOutUpRight),
        "Open",
    )
    .disabled(!available(action, cx))
    .tooltip(format!("Runs {action}"))
    .on_click(move |_, window, cx| {
        if let Ok(action) = cx.build_action(action, None) {
            window.dispatch_action(action, cx);
        }
    });
    Some(sample("Opens the real surface", button))
}

/// The samples of the surface called `name`: open it, then its components.
fn samples(name: &str, cx: &mut Context<ComponentGallery>) -> Vec<Sample> {
    let Some(surface) = surface(name) else {
        return Vec::new();
    };
    let weak = cx.entity().downgrade();
    let links = surface.composes.iter().map(|component| {
        reveal_button(
            named(&format!("gallery-composes:{}", surface.name), component),
            component,
            weak.clone(),
        )
    });
    let composes = h_flex()
        .max_w(gpui_kit::px(LINKS_WIDTH))
        .flex_wrap()
        .gap_1()
        .children(links);
    open_button(surface, cx)
        .into_iter()
        .chain([sample("Built from; click to show each card", composes)])
        .collect()
}

macro_rules! surface_samples {
    ($($builder:ident => $name:literal),* $(,)?) => {
        $(
            pub(crate) fn $builder(
                _: &ComponentGallery,
                _: &Tokens,
                _: &mut Window,
                cx: &mut Context<ComponentGallery>,
            ) -> Vec<Sample> {
                samples($name, cx)
            }
        )*
    };
}

surface_samples!(
    window => "shell::window",
    toolbar => "shell::toolbar",
    navigation_rail => "shell::navigation_rail",
    status_bar => "shell::status_bar",
    command_palette => "shell::command_palette",
    help => "shell::help",
    settings_dialog => "shell::settings_dialog",
    about_dialog => "shell::about_dialog",
    report_issue => "shell::report_issue",
    theme_designer => "shell::theme_designer",
    toasts => "shell::toasts",
    chat_panel => "shell::chat_panel",
    chat_canvas => "shell::chat_canvas",
    search_view => "shell::search_view",
    log_view => "shell::log_view",
);
