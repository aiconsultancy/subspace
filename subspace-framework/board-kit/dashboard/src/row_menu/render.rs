//! Builds a [`GroupedMenu`] into GPUI Kit's native `PopupMenu`.

use gpui_kit::component::Icon;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{Action, Context, SharedString, Styled as _, Window};

use super::groups::{GroupedMenu, MenuCommand, Run, Slot, layout};
use crate::tokens::Tokens;

/// Builds `grouped` into `menu`: the groups in order with a separator
/// between them, each command with its Lucide glyph and key, a disabled
/// command's reason on the line under it, and the Remove group's glyphs in
/// the theme's danger colour.
pub fn build(
    menu: PopupMenu,
    grouped: GroupedMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let slots = layout(&grouped.commands, grouped.extension.is_some());
    let mut commands: Vec<Option<MenuCommand>> = grouped.commands.into_iter().map(Some).collect();
    let mut extension = grouped.extension;
    slots.into_iter().fold(menu, |menu, slot| match slot {
        Slot::Separator => menu.separator(),
        Slot::Reason(reason) => menu.label(reason),
        Slot::Command(index) => match commands.get_mut(index).and_then(Option::take) {
            Some(command) => add_command(menu, command, window, cx),
            None => menu,
        },
        Slot::Extension => match extension.take() {
            Some(extend) => extend(menu, window, cx),
            None => menu,
        },
    })
}

fn add_command(
    menu: PopupMenu,
    command: MenuCommand,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let MenuCommand {
        group,
        label,
        icon,
        unavailable,
        key,
        run,
    } = command;
    let enabled = unavailable.is_none();
    let glyph = glyph(icon, enabled && group.is_danger(), &Tokens::current(cx));
    match run {
        Run::Submenu(build) if enabled => {
            menu.submenu_with_icon(Some(glyph), label, window, cx, move |menu, window, cx| {
                build(menu, window, cx)
            })
        }
        run => menu.item(item(label, glyph, run, key, enabled)),
    }
}

/// The command's Lucide glyph, in the danger colour for a command that
/// removes or leaves something.
fn glyph(icon: subspace_lucide::IconName, danger: bool, tokens: &Tokens) -> Icon {
    let glyph = Icon::from(icon);
    match danger {
        true => glyph.text_color(tokens.red),
        false => glyph,
    }
}

/// A native item: the label stays its accessible name. A disabled submenu
/// shows as a disabled item, since there is nothing to choose in it.
fn item(
    label: SharedString,
    glyph: Icon,
    run: Run,
    key: Option<Box<dyn Action>>,
    enabled: bool,
) -> PopupMenuItem {
    let item = PopupMenuItem::new(label).icon(glyph).disabled(!enabled);
    // An item's action shows its key; with a click handler too, the
    // handler is what runs.
    let item = match key {
        Some(key) => item.action(key),
        None => item,
    };
    match run {
        Run::Click(run) => item.on_click(move |_, window, cx| run(window, cx)),
        Run::Action(action) => item.action(action),
        Run::Submenu(_) => item,
    }
}
