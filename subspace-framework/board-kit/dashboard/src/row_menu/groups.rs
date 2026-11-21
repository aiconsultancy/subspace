//! The seven groups every context menu is built from, in the order every
//! menu shows them. A board fills groups in; it never orders them, and a
//! group with nothing in it is left out.

use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Action, App, Context, SharedString, Window};
use subspace_lucide::IconName;

use crate::Callback;

/// Builds a submenu's entries in the submenu's own context.
pub(super) type SubmenuBuilder =
    Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Entries a board adds straight to the menu through the older
/// `ListDashboard` hooks. They land at the end of [`MenuGroup::Act`].
pub(crate) type Extension =
    Box<dyn FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// A context menu's groups, in the order every menu shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuGroup {
    /// The default action first ("Open", with Enter), then "Open in …".
    Open,
    /// The item's own verbs, with submenus for choices.
    Act,
    /// Copy (the selected text), Copy link, Copy title, Copy id.
    Copy,
    /// Add to chat, Add selection to chat.
    Chat,
    /// The container's commands, such as folding a group or closing a tab.
    View,
    /// Commands that remove or leave something, tinted as danger. An
    /// ellipsis in the label means the command asks first.
    Remove,
    /// Settings for the place, such as "Space settings…".
    Place,
}

impl MenuGroup {
    /// Every group, in menu order.
    pub const ORDER: [Self; 7] = [
        Self::Open,
        Self::Act,
        Self::Copy,
        Self::Chat,
        Self::View,
        Self::Remove,
        Self::Place,
    ];

    /// Whether the group's commands remove or leave something, which the
    /// menu tints in the theme's danger colour.
    pub fn is_danger(self) -> bool {
        self == Self::Remove
    }
}

/// What choosing a command does.
pub(super) enum Run {
    Click(Callback),
    /// Dispatches the action, as its key does.
    Action(Box<dyn Action>),
    Submenu(SubmenuBuilder),
}

/// One command in a context menu: its group, words and Lucide glyph, and
/// what it does. A command that exists but cannot run now stays in the
/// menu, disabled, with one line saying why.
pub struct MenuCommand {
    pub(super) group: MenuGroup,
    pub(super) label: SharedString,
    pub(super) icon: IconName,
    pub(super) unavailable: Option<SharedString>,
    pub(super) key: Option<Box<dyn Action>>,
    pub(super) run: Run,
}

impl MenuCommand {
    /// A command that calls `run`. Capture a weak handle to act on a board.
    pub fn new(
        group: MenuGroup,
        label: impl Into<SharedString>,
        icon: IconName,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self::with_run(group, label.into(), icon, Run::Click(Rc::new(run)))
    }

    /// A command that dispatches `action`, as its key does, and shows that
    /// key.
    pub fn action(
        group: MenuGroup,
        label: impl Into<SharedString>,
        icon: IconName,
        action: impl Action,
    ) -> Self {
        Self::with_run(group, label.into(), icon, Run::Action(Box::new(action)))
    }

    /// A submenu of choices, which `build` fills as it opens.
    pub fn submenu(
        group: MenuGroup,
        label: impl Into<SharedString>,
        icon: IconName,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        Self::with_run(group, label.into(), icon, Run::Submenu(Rc::new(build)))
    }

    fn with_run(group: MenuGroup, label: SharedString, icon: IconName, run: Run) -> Self {
        Self {
            group,
            label,
            icon,
            unavailable: None,
            key: None,
            run,
        }
    }

    /// Shows the key bound to `action` beside the label, for a command
    /// that does what that key does.
    pub fn key(mut self, action: impl Action) -> Self {
        self.key = Some(Box::new(action));
        self
    }

    /// Disables the command when `disabled`, with `reason` on a line under
    /// it, such as "The last open Space can't be closed."
    pub fn disabled(mut self, disabled: bool, reason: impl Into<SharedString>) -> Self {
        self.unavailable = disabled.then(|| reason.into());
        self
    }

    pub fn group(&self) -> MenuGroup {
        self.group
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Why the command cannot run now, when it is disabled.
    pub fn reason(&self) -> Option<&SharedString> {
        self.unavailable.as_ref()
    }
}

/// The commands of one context menu, which [`super::build`] shows in
/// [`MenuGroup::ORDER`] with a separator between groups that have any.
#[derive(Default)]
pub struct GroupedMenu {
    pub(super) commands: Vec<MenuCommand>,
    pub(super) extension: Option<Extension>,
}

impl GroupedMenu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `command` after the commands already in its group.
    pub fn with(mut self, command: MenuCommand) -> Self {
        self.commands.push(command);
        self
    }

    pub fn push(&mut self, command: MenuCommand) {
        self.commands.push(command);
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.extension.is_none()
    }

    /// Entries added straight to the menu at the end of [`MenuGroup::Act`],
    /// for the older `ListDashboard` hooks.
    pub(crate) fn extend_act(mut self, extension: Extension) -> Self {
        self.extension = Some(extension);
        self
    }

    /// What the menu shows, line by line, without building it.
    pub fn outline(&self) -> Vec<MenuLine> {
        layout(&self.commands, self.extension.is_some())
            .into_iter()
            .map(|slot| outline_line(&self.commands, slot))
            .collect()
    }
}

impl Extend<MenuCommand> for GroupedMenu {
    fn extend<I: IntoIterator<Item = MenuCommand>>(&mut self, commands: I) {
        self.commands.extend(commands);
    }
}

/// One line of a menu's [`GroupedMenu::outline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuLine {
    Separator,
    Command {
        group: MenuGroup,
        label: SharedString,
        enabled: bool,
        /// Tinted in the theme's danger colour.
        danger: bool,
    },
    /// Why the command above cannot run now.
    Reason(SharedString),
    /// Entries a board adds straight to the menu.
    Extension,
}

/// A line of the menu, by the index of its command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Slot {
    Separator,
    Command(usize),
    Reason(SharedString),
    Extension,
}

/// The menu's lines: the groups in order, each command followed by its
/// reason when it is disabled, and a separator between groups that have
/// lines. `extension` adds the board's own entries at the end of Act.
pub(super) fn layout(commands: &[MenuCommand], extension: bool) -> Vec<Slot> {
    let mut slots = Vec::new();
    for group in MenuGroup::ORDER {
        let extended = extension && group == MenuGroup::Act;
        let members: Vec<usize> = (0..commands.len())
            .filter(|index| commands[*index].group == group)
            .collect();
        if members.is_empty() && !extended {
            continue;
        }
        if !slots.is_empty() {
            slots.push(Slot::Separator);
        }
        for index in members {
            slots.push(Slot::Command(index));
            let reason = commands[index].unavailable.clone();
            slots.extend(
                reason
                    .filter(|why| !why.trim().is_empty())
                    .map(Slot::Reason),
            );
        }
        if extended {
            slots.push(Slot::Extension);
        }
    }
    slots
}

fn outline_line(commands: &[MenuCommand], slot: Slot) -> MenuLine {
    match slot {
        Slot::Separator => MenuLine::Separator,
        Slot::Reason(reason) => MenuLine::Reason(reason),
        Slot::Extension => MenuLine::Extension,
        Slot::Command(index) => {
            let command = &commands[index];
            let enabled = command.unavailable.is_none();
            MenuLine::Command {
                group: command.group,
                label: command.label.clone(),
                enabled,
                danger: enabled && command.group.is_danger(),
            }
        }
    }
}
