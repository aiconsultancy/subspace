//! The shell surfaces: the action that opens each one and the gallery
//! names of the components it is built from.

/// One shell surface: its gallery name, the shell action that opens it and
/// the gallery names of the components it is built from.
pub(crate) struct Surface {
    pub(crate) name: &'static str,
    pub(crate) action: Option<&'static str>,
    pub(crate) composes: &'static [&'static str],
}

pub(crate) const SURFACES: [Surface; 15] = [
    Surface {
        name: "shell::window",
        action: None,
        composes: &[
            "gpui_kit::component::resizable::ResizablePanelGroup",
            "gpui_kit::component::Root",
        ],
    },
    Surface {
        name: "shell::toolbar",
        action: None,
        composes: &[
            "glass::nav_button",
            "overflow_toolbar::OverflowToolbar",
            "icon_button::IconButton",
            "badge::badge",
        ],
    },
    Surface {
        name: "shell::navigation_rail",
        action: Some("subspace_app::ToggleNavigationMode"),
        composes: &["glass::nav_button", "badge::badge"],
    },
    Surface {
        name: "shell::status_bar",
        action: None,
        composes: &[
            "icon_button::IconButton",
            "gpui_kit::component::spinner::Spinner",
            "gpui_kit::component::status_bar::StatusBar",
        ],
    },
    Surface {
        name: "shell::command_palette",
        action: Some("subspace_app::TogglePalette"),
        composes: &["gpui_kit::component::command::Command", "badge::badge"],
    },
    Surface {
        name: "shell::help",
        action: Some("subspace_app::ToggleHelp"),
        composes: &[
            "rich::rich_text",
            "gpui_kit::component::collapsible::Collapsible",
            "gpui_kit::component::dock::DockArea",
        ],
    },
    Surface {
        name: "shell::settings_dialog",
        action: Some("subspace_app::OpenSettings"),
        composes: &[
            "fields::form_field",
            "fields::heading",
            "fields::note",
            "toggle::toggle_row",
            "secret::secret_field",
            "anchored::select",
            "gpui_kit::component::tree::Tree",
            "gpui_kit::component::dialog::AlertDialog",
        ],
    },
    Surface {
        name: "shell::about_dialog",
        action: Some("subspace_app::OpenAbout"),
        composes: &[
            "rich::plain_text",
            "copy_details::CopyDetails",
            "links::external",
            "gpui_kit::component::group_box::GroupBox",
            "gpui_kit::component::progress::Progress",
            "gpui_kit::component::dialog::Dialog",
        ],
    },
    Surface {
        name: "shell::report_issue",
        action: Some("subspace_app::OpenReportIssue"),
        composes: &[
            "toggle::toggle_row",
            "gpui_kit::component::input::Textarea",
            "action::action_button",
            "gpui_kit::component::collapsible::Collapsible",
        ],
    },
    Surface {
        name: "shell::theme_designer",
        action: Some("subspace_app::ToggleJig"),
        composes: &[
            "gpui_kit::component::accordion::Accordion",
            "segmented::control",
            "toggle::toggle_row",
            "gpui_kit::component::slider::Slider",
            "tokens::Tokens",
        ],
    },
    Surface {
        name: "shell::toasts",
        action: None,
        composes: &["Toast", "action::icon_button", "links::external"],
    },
    Surface {
        name: "shell::chat_panel",
        action: Some("subspace_app::ToggleChat"),
        composes: &[
            "chat_input::ChatInput",
            "chat_input::completion::CompletionList",
            "chat_bubble::bubble",
            "tabs::tab_bar",
            "gpui_kit::component::attachment::Attachment",
        ],
    },
    Surface {
        name: "shell::chat_canvas",
        action: None,
        composes: &["chart::Chart", "action::action_button"],
    },
    Surface {
        name: "shell::search_view",
        action: Some("subspace_app::OpenSearchView"),
        composes: &[
            "surface_header::SurfaceHeader",
            "fields::input",
            "icon_button::latched",
            "toggle::switch",
        ],
    },
    Surface {
        name: "shell::log_view",
        action: Some("subspace_app::OpenLogView"),
        composes: &[
            "surface_header::SurfaceHeader",
            "tabs::tab_bar",
            "overflow_toolbar::OverflowToolbar",
            "icon_button::latched",
        ],
    },
];
