//! Stop controls for running requests and their verified agent replies.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{AnyElement, IntoElement as _, ParentElement as _, SharedString, div};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::{anchored, icon_button::IconButton, panel::IconName};

use super::{Entry, RowContext, at_button};
use crate::reactions::choices::{StopGroup, choices};

struct Stop {
    msgid: String,
    group: StopGroup,
    conversation: gpui_kit::WeakEntity<crate::conversation::Conversation>,
}

impl Stop {
    fn for_entry(entry: &Entry, context: &RowContext) -> Option<Self> {
        let msgid = entry.line.id.clone().filter(|_| context.actions.react)?;
        let agent = context
            .stops
            .get(&entry.seq)
            .filter(|agent| agent.running)?;
        let summaries = context
            .reactions
            .get(&entry.seq)
            .map_or(&[][..], Vec::as_slice);
        Some(Self {
            msgid,
            group: choices(summaries, Some(&agent.ship)).stop?,
            conversation: context.conversation.clone(),
        })
    }

    fn button(&self, id: String) -> IconButton {
        icon_button(
            SharedString::from(id),
            Intent::Quiet,
            IconName::Square,
            self.group.label(),
        )
    }

    fn menu(&self, menu: PopupMenu) -> PopupMenu {
        crate::transcript::reactions::menu_items(
            menu.label(self.group.label()),
            &self.group.choices,
            (&self.msgid, Some(&self.group.ship)),
            &self.conversation,
        )
    }
}

pub(super) fn toolbar(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    let stop = Stop::for_entry(entry, context)?;
    let anchor = context.react_anchors.stop.clone();
    let menu = anchor.clone();
    let button = stop
        .button(format!("irc-line:{}:stop", entry.seq))
        .on_click(move |_, window, cx| {
            menu.update(cx, |anchor, cx| {
                anchor.show_menu_with(window, cx, |menu, _, _| stop.menu(menu));
            });
        });
    Some(at_button(div().child(button), entry.seq, context, anchor))
}

/// The inspector owns its trigger so Stop also works when only the
/// inspector is mounted, such as in Comms.
pub(in crate::transcript) fn thread(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    let stop = Stop::for_entry(entry, context)?;
    let button = stop.button(format!("irc-thread-line:{}:stop", entry.seq));
    Some(anchored::dropdown_menu(button, move |menu, _, _| stop.menu(menu)).into_any_element())
}
