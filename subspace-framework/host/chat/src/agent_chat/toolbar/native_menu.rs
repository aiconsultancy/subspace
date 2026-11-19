//! Native control-menu contents shared by inline triggers and overflow entries.

use super::labels::{
    current_model, effort_checked, effort_options, effort_value, mode_label, permission_label,
};
use super::overflow::Control;
use super::{ChatModeOption, ChatToolbar, ToolbarEvent, ToolbarModel};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{Context, SharedString, WeakEntity, Window};

fn item(
    label: impl Into<SharedString>,
    checked: bool,
    toolbar: WeakEntity<ChatToolbar>,
    event: ToolbarEvent,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .checked(checked)
        .on_click(move |_, _, cx| {
            let event = event.clone();
            let _ = toolbar.update(cx, |_, cx| cx.emit(event));
        })
}

fn permission_items(
    menu: PopupMenu,
    toolbar: WeakEntity<ChatToolbar>,
    approve_all: bool,
) -> PopupMenu {
    menu.item(item(
        permission_label(false),
        !approve_all,
        toolbar.clone(),
        ToolbarEvent::SetApproveAll(false),
    ))
    .item(item(
        permission_label(true),
        approve_all,
        toolbar,
        ToolbarEvent::SetApproveAll(true),
    ))
}

pub(super) struct NativeMenu {
    control: Control,
    model: ToolbarModel,
    toolbar: WeakEntity<ChatToolbar>,
}

impl NativeMenu {
    pub(super) fn new(
        control: Control,
        model: ToolbarModel,
        toolbar: WeakEntity<ChatToolbar>,
    ) -> Self {
        Self {
            control,
            model,
            toolbar,
        }
    }

    pub(super) fn build(
        self,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        match self.control {
            Control::Context => self.context_items(menu),
            Control::Mode => self.mode_menu(menu, window, cx),
            Control::Agent => self.agent_menu(menu),
            Control::View => self.view_menu(menu),
            Control::Reasoning => self.reasoning_menu(menu),
            Control::Model | Control::Usage => menu,
        }
    }

    fn agent_menu(&self, menu: PopupMenu) -> PopupMenu {
        self.model.agents.iter().fold(menu, |menu, agent| {
            menu.item(item(
                agent.label.clone(),
                Some(&agent.id) == self.model.agent.as_ref(),
                self.toolbar.clone(),
                ToolbarEvent::SetAgent(agent.id.clone()),
            ))
        })
    }

    fn view_menu(&self, menu: PopupMenu) -> PopupMenu {
        menu.item(item(
            "Show tool activity",
            self.model.show_tools,
            self.toolbar.clone(),
            ToolbarEvent::ToggleTools,
        ))
        .item(item(
            "Show thinking",
            self.model.show_thinking,
            self.toolbar.clone(),
            ToolbarEvent::ToggleThinking,
        ))
    }

    fn reasoning_menu(&self, menu: PopupMenu) -> PopupMenu {
        let efforts = effort_options(current_model(
            &self.model.models,
            self.model.model.as_deref(),
        ));
        efforts.iter().fold(menu, |menu, effort| {
            menu.item(item(
                effort.clone(),
                effort_checked(effort, self.model.effort.as_deref()),
                self.toolbar.clone(),
                ToolbarEvent::SetEffort(effort_value(effort)),
            ))
        })
    }

    fn context_items(&self, menu: PopupMenu) -> PopupMenu {
        let toolbar = self.toolbar.clone();
        menu.item(item(
            "Attach file",
            false,
            toolbar.clone(),
            ToolbarEvent::AttachFile,
        ))
        .item(
            item(
                "Add current selection",
                false,
                toolbar.clone(),
                ToolbarEvent::AddSelection,
            )
            .disabled(!self.model.has_selection),
        )
        .separator()
        .item(item(
            "Auto context",
            self.model.auto_context,
            toolbar,
            ToolbarEvent::ToggleAutoContext,
        ))
    }

    fn mode_menu(
        &self,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let toolbar = self.toolbar.clone();
        let approve_all = self.model.approve_all;
        let menu = ChatModeOption::ALL.iter().fold(menu, |menu, &option| {
            menu.item(item(
                mode_label(option),
                option == self.model.mode,
                toolbar.clone(),
                ToolbarEvent::SetMode(option),
            ))
        });
        menu.separator()
            .submenu("Permissions", window, cx, move |sub, _, _| {
                permission_items(sub, toolbar.clone(), approve_all)
            })
    }
}
