//! Shared overflow projections for the shell's existing global controls.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{Action, App, Context, IntoElement as _, WeakEntity, Window, div};
use subspace_dashboard::PaletteMode;
use subspace_dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_lucide::IconName;

#[cfg(feature = "report-issue")]
use super::super::OpenReportIssue;
use super::super::{
    OpenAbout, OpenSearchView, OpenSettings, Reload, SubspaceApp, ToggleChat, ToggleHelp,
    ToggleJig, TogglePalette, ToggleTerminal, ToggleWrap, toggle_wrap,
};
use super::TAB_WIDTH_GUESS;
#[cfg(feature = "report-issue")]
use super::chrome::report_issue_chrome;
use super::chrome::{
    about_chrome, about_presentation, help_chrome, jig_chrome, reload_chrome, settings_chrome,
    wrap_chrome,
};
use crate::toolbar_labels::ToolbarControl;

struct GlobalControl {
    control: ToolbarControl,
    icon: IconName,
    action: fn() -> Box<dyn Action>,
}

const CONTROLS: &[GlobalControl] = &[
    GlobalControl {
        control: ToolbarControl::Chat,
        icon: IconName::Bot,
        action: || Box::new(ToggleChat),
    },
    GlobalControl {
        control: ToolbarControl::Terminal,
        icon: IconName::SquareTerminal,
        action: || Box::new(ToggleTerminal),
    },
    GlobalControl {
        control: ToolbarControl::Search,
        icon: IconName::Search,
        action: || Box::new(OpenSearchView),
    },
    GlobalControl {
        control: ToolbarControl::Commands,
        icon: IconName::Command,
        action: || Box::new(TogglePalette),
    },
    GlobalControl {
        control: ToolbarControl::Reload,
        icon: IconName::RotateCw,
        action: || Box::new(Reload),
    },
    GlobalControl {
        control: ToolbarControl::Wrap,
        icon: IconName::TextWrap,
        action: || Box::new(ToggleWrap),
    },
    GlobalControl {
        control: ToolbarControl::Jig,
        icon: IconName::Eye,
        action: || Box::new(ToggleJig),
    },
    GlobalControl {
        control: ToolbarControl::Settings,
        icon: IconName::Settings,
        action: || Box::new(OpenSettings),
    },
    GlobalControl {
        control: ToolbarControl::Help,
        icon: IconName::CircleQuestionMark,
        action: || Box::new(ToggleHelp),
    },
    GlobalControl {
        control: ToolbarControl::About,
        icon: IconName::Info,
        action: || Box::new(OpenAbout),
    },
    #[cfg(feature = "report-issue")]
    GlobalControl {
        control: ToolbarControl::ReportIssue,
        icon: IconName::FaceSlightlyFrowning,
        action: || Box::new(OpenReportIssue),
    },
];

/// One global control's menu row for this frame, shared by the overflow
/// chevron and the collapsed toolbar menu so both run the same command.
#[derive(Clone)]
pub(super) struct GlobalItem {
    control: ToolbarControl,
    icon: IconName,
    label: &'static str,
    selected: bool,
    disabled: bool,
    action: fn() -> Box<dyn Action>,
    owner: WeakEntity<SubspaceApp>,
}

impl GlobalItem {
    pub(super) fn menu_item(&self) -> PopupMenuItem {
        let owner = self.owner.clone();
        let control = self.control;
        PopupMenuItem::new(self.label)
            .icon(self.icon)
            .checked(self.selected)
            .disabled(self.disabled)
            .action((self.action)())
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |app, cx| app.activate_global(control, window, cx));
            })
    }
}

impl SubspaceApp {
    pub(super) fn sync_global_units(&self, busy: bool, cx: &mut Context<Self>) {
        let items = CONTROLS
            .iter()
            .filter(|spec| spec.control.offered(self.terminal.is_some(), cx))
            .map(|spec| self.global_item(spec, busy, cx))
            .collect();
        let units = global_units(items, busy, crate::toolbar_labels::collapsed(cx));
        self.globals_toolbar.update(cx, |toolbar, cx| {
            toolbar.set_units(units, cx);
        });
    }

    fn global_item(&self, spec: &GlobalControl, busy: bool, cx: &Context<Self>) -> GlobalItem {
        let control = spec.control;
        let label = match control {
            ToolbarControl::Jig => "Theme designer",
            ToolbarControl::Reload if busy => "Reloading…",
            _ => control.label(),
        };
        let (icon, label) = if control == ToolbarControl::About {
            about_presentation(crate::app::available_update_version(cx).is_some())
        } else {
            (spec.icon, label)
        };
        GlobalItem {
            control,
            icon,
            label,
            selected: self.global_selected(control, cx),
            disabled: control == ToolbarControl::Reload && busy,
            action: spec.action,
            owner: cx.weak_entity(),
        }
    }
}

/// Every control inline, or one menu that holds them all.
fn global_units(items: Vec<GlobalItem>, busy: bool, collapsed: bool) -> Vec<OverflowUnit> {
    if collapsed {
        return vec![super::menu::menu_unit(items)];
    }
    items
        .into_iter()
        .map(|item| global_unit(item, busy))
        .collect()
}

fn global_unit(item: GlobalItem, busy: bool) -> OverflowUnit {
    let control = item.control;
    let owner = item.owner.clone();
    OverflowUnit::inline(
        control.key(),
        TAB_WIDTH_GUESS,
        move |_, cx| {
            owner
                .update(cx, |app, cx| app.global_inline(control, busy, cx))
                .unwrap_or_else(|_| div().into_any_element())
        },
        vec![PopupEntry::new(move || item.menu_item())],
    )
}

impl SubspaceApp {
    fn global_selected(&self, control: ToolbarControl, cx: &App) -> bool {
        match control {
            ToolbarControl::Chat => self.chat_open,
            ToolbarControl::Terminal => self.shown_terminal().is_some(),
            ToolbarControl::Search => self.main.search().is_some(),
            ToolbarControl::Wrap => subspace_dashboard::wrap(cx),
            ToolbarControl::Jig => self.jig_open.get(),
            ToolbarControl::Help => self.help_open,
            _ => false,
        }
    }

    fn global_inline(
        &self,
        control: ToolbarControl,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        match control {
            ToolbarControl::Chat => self.chat_button(cx).into_any_element(),
            ToolbarControl::Terminal => self.terminal_button(cx).into_any_element(),
            ToolbarControl::Search => self.search_button(cx).into_any_element(),
            ToolbarControl::Commands => Self::palette_button(cx).into_any_element(),
            ToolbarControl::Reload => reload_chrome(busy, cx),
            ToolbarControl::Wrap => wrap_chrome(subspace_dashboard::wrap(cx), cx),
            ToolbarControl::Jig => jig_chrome(self.jig_open.get(), cx),
            ToolbarControl::Settings => settings_chrome(cx),
            ToolbarControl::Help => help_chrome(self.help_open, cx),
            ToolbarControl::About => about_chrome(cx),
            #[cfg(feature = "report-issue")]
            ToolbarControl::ReportIssue => report_issue_chrome(cx),
        }
    }

    fn activate_global(
        &mut self,
        control: ToolbarControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match control {
            ToolbarControl::Chat => self.toggle_chat(&ToggleChat, window, cx),
            ToolbarControl::Terminal => {
                self.change_terminal(super::super::terminal_dock::Change::Toggle, window, cx);
            }
            ToolbarControl::Search => self.open_full_text(&OpenSearchView, window, cx),
            ToolbarControl::Commands => self.open_palette(PaletteMode::Commands, window, cx),
            ToolbarControl::Reload => self.reload(&Reload, window, cx),
            ToolbarControl::Wrap => toggle_wrap(cx),
            ToolbarControl::Jig => self.set_jig(!self.jig_open.get(), window, cx),
            ToolbarControl::Settings => self.open_settings(&OpenSettings, window, cx),
            ToolbarControl::Help => self.toggle_help(&ToggleHelp, window, cx),
            ToolbarControl::About => self.open_about(&OpenAbout, window, cx),
            #[cfg(feature = "report-issue")]
            ToolbarControl::ReportIssue => self.open_report_issue(&OpenReportIssue, window, cx),
        }
    }
}
