//! GPUI Kit layout components, in GPUI Kit's documentation order, with
//! AlertDialog, HoverCard, TitleBar and FocusTrap from its story gallery.

mod overlays;
mod regions;
#[cfg(test)]
mod tests;

use gpui_kit::component::ResizableState;
use gpui_kit::component::input::InputState;
use gpui_kit::{App, AppContext as _, Entity, FocusHandle, Window};

use super::kit_source;
use crate::catalog::{Item, Kit};
use crate::samples::popover::PopoverHost;

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "gpui_kit::component::description_list::DescriptionList",
        source: kit_source!("description_list.rs"),
        used_in: "Not used in Subspace yet; About lists build metadata in its own grid.",
        samples: regions::description_lists,
        kit: Some(Kit {
            paths: &["description_list::DescriptionList"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::group_box::GroupBox",
        source: kit_source!("group_box.rs"),
        used_in: "The About dialog's update panel.",
        samples: regions::group_boxes,
        kit: Some(Kit {
            paths: &["group_box::GroupBox"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::Root",
        source: kit_source!("root.rs (re-exported from gpui-base)"),
        used_in: "The first view of every Subspace window, through gpui_kit::open_window.",
        samples: overlays::roots,
        kit: Some(Kit {
            paths: &["Root", "gpui_kit::open_window("],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::Theme",
        source: kit_source!("theme/mod.rs"),
        used_in: "Every themed colour; Subspace maps it to tokens::Tokens.",
        samples: overlays::themes,
        kit: Some(Kit {
            paths: &["Theme", "theme::Theme", "ActiveTheme"],
            used: true,
            prefer: Some("tokens::Tokens"),
        }),
    },
    Item {
        name: "gpui_kit::component::dialog::Dialog",
        source: kit_source!("dialog/dialog.rs"),
        used_in: "About and Settings; board dialogs use the shared dialog frame.",
        samples: overlays::dialogs,
        kit: Some(Kit {
            paths: &["dialog::Dialog", ".open_dialog("],
            used: true,
            prefer: Some("dialog::frame"),
        }),
    },
    Item {
        name: "gpui_kit::component::dialog::AlertDialog",
        source: kit_source!("dialog/alert_dialog.rs"),
        used_in: "The unsaved Settings confirmation.",
        samples: overlays::alert_dialogs,
        kit: Some(Kit {
            paths: &["dialog::AlertDialog", ".open_alert_dialog("],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::notification::Notification",
        source: kit_source!("notification.rs"),
        used_in: "In-app confirmations such as Copy details; board messages use the shell's Toast stack.",
        samples: overlays::notifications,
        kit: Some(Kit {
            paths: &["notification::Notification", ".push_notification("],
            used: true,
            prefer: Some("Toast"),
        }),
    },
    Item {
        name: "gpui_kit::component::popover::Popover",
        source: kit_source!("popover.rs"),
        used_in: "Actions filters use the native form popover through anchored::form_popover.",
        samples: overlays::popovers,
        kit: Some(Kit {
            paths: &["popover::Popover"],
            used: true,
            prefer: Some("anchored::form_popover"),
        }),
    },
    Item {
        name: "gpui_kit::component::hover_card::HoverCard",
        source: kit_source!("hover_card.rs"),
        used_in: "Not used in Subspace yet; IRC issue cards use Tooltip::element.",
        samples: overlays::hover_cards,
        kit: Some(Kit {
            paths: &["hover_card::HoverCard"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::resizable::ResizablePanelGroup",
        source: kit_source!("resizable.rs (gpui-base)"),
        used_in: "The shell's chat and help split and the navigation rail width.",
        samples: regions::resizables,
        kit: Some(Kit {
            paths: &[
                "resizable::ResizablePanelGroup",
                "h_resizable",
                "v_resizable",
                "resizable_panel",
                "ResizableState",
            ],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::scroll::Scrollable",
        source: kit_source!("scroll/scrollable.rs"),
        used_in: "Scrollbars in Settings and board viewers.",
        samples: regions::scrollables,
        kit: Some(Kit {
            paths: &[
                "scroll::Scrollbar",
                "scroll::Scrollable",
                "scroll::ScrollableElement",
                ".overflow_y_scrollbar(",
            ],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::sheet::Sheet",
        source: kit_source!("sheet.rs"),
        used_in: "Not used in Subspace yet.",
        samples: overlays::sheets,
        kit: Some(Kit {
            paths: &["sheet::Sheet", ".open_sheet("],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::sidebar::Sidebar",
        source: kit_source!("sidebar/mod.rs"),
        used_in: "The gallery's own navigation; Settings and the rail use shell compositions.",
        samples: regions::sidebars,
        kit: Some(Kit {
            paths: &["sidebar::Sidebar"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::status_bar::StatusBar",
        source: kit_source!("status_bar.rs"),
        used_in: "The window status bar.",
        samples: regions::status_bars,
        kit: Some(Kit {
            paths: &["status_bar::StatusBar"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::toolbar::Toolbar",
        source: kit_source!("toolbar.rs"),
        used_in: "Not used in Subspace; toolbars use overflow_toolbar::OverflowToolbar.",
        samples: regions::toolbars,
        kit: Some(Kit {
            paths: &["toolbar::Toolbar"],
            used: false,
            prefer: Some("overflow_toolbar::OverflowToolbar"),
        }),
    },
    Item {
        name: "gpui_kit::component::TitleBar",
        source: kit_source!("title_bar.rs"),
        used_in: "Not used in Subspace: the shell draws its own title bar. The sample is covered so that dragging it cannot move the window.",
        samples: regions::title_bars,
        kit: Some(Kit {
            paths: &["TitleBar"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::FocusTrapElement",
        source: kit_source!("focus_trap.rs (gpui-base)"),
        used_in: "Not used directly by the Framework outside the gallery; native dialogs manage their own focus.",
        samples: regions::focus_traps,
        kit: Some(Kit {
            paths: &["FocusTrapElement", ".focus_trap("],
            used: false,
            prefer: None,
        }),
    },
];

/// Retained entities of the layout samples.
pub(crate) struct State {
    pub(crate) popover: PopoverHost,
    pub(crate) panes: Entity<ResizableState>,
    pub(crate) trap: FocusHandle,
    pub(crate) trap_first: Entity<InputState>,
    pub(crate) trap_second: Entity<InputState>,
}

impl State {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            popover: PopoverHost::new(cx),
            panes: cx.new(|_| ResizableState::default()),
            trap: cx.focus_handle(),
            trap_first: cx.new(|cx| InputState::new(window, cx).placeholder("First field")),
            trap_second: cx.new(|cx| InputState::new(window, cx).placeholder("Second field")),
        }
    }
}
