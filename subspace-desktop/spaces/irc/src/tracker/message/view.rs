//! The action dialog over the conversation: the quoted message, the
//! editable title, the project or issue choice, Cancel and the button
//! that writes.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    div,
};
use subspace_framework::dashboard::action::{Intent, dialog_button, menu_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::fields::{self, FieldLayout, LabelledInput};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, anchored, rich};

use super::{CONTEXT, CancelTrackerDialog, Dialog, Kind, QUOTE_CHARS, choose, close, confirm};
use crate::conversation::Conversation;
use crate::tracker::plan::clip;
use crate::tracker::quote::Quoted;

/// The open dialog over the conversation.
pub(crate) fn render(
    conversation: &Conversation,
    cx: &mut Context<Conversation>,
) -> Option<AnyElement> {
    let dialog = conversation.tracker.dialog.as_ref()?;
    let tokens = Tokens::current(cx);
    let (heading, label, button, icon) = match dialog.kind {
        Kind::MakeIssue => (
            "Make an issue",
            "Project",
            "Create issue",
            IconName::CircleDot,
        ),
        Kind::RecordDecision => (
            "Record a decision",
            "Issue",
            "Record decision",
            IconName::Scale,
        ),
    };
    let heading = format!(
        "{heading} from {}'s message",
        clip(&dialog.quoted.author, 32)
    );
    let busy = conversation.tracker.busy;
    let heading_label = SharedString::from(heading.clone());
    let frame = dialog::frame(&tokens, DialogWidth::D540, heading)
        .id("irc-tracker-dialog")
        .role(Role::Dialog)
        .aria_label(heading_label.clone())
        .test_support()
        .key_context(CONTEXT)
        .on_action(cx.listener(|this, _: &CancelTrackerDialog, window, cx| close(this, window, cx)))
        .child(quote(&dialog.quoted, &tokens))
        .child(fields::labelled_input(
            LabelledInput::new(&dialog.title, "Title", FieldLayout::Stacked)
                .error(dialog.error.clone()),
            &tokens,
        ))
        .child(picker(dialog, label, cx.entity().downgrade(), &tokens))
        .child(
            dialog::footer()
                .child(
                    dialog_button(
                        "irc-tracker-cancel",
                        Intent::Neutral,
                        Some(IconName::X),
                        "Cancel",
                    )
                    .on_click(cx.listener(|this, _, window, cx| close(this, window, cx))),
                )
                .child(
                    dialog_button("irc-tracker-confirm", Intent::Primary, Some(icon), button)
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| confirm(this, window, cx))),
                ),
        );
    Some(dialog::backdrop(&tokens, frame))
}

/// The message as the dialog quotes it: who, then the start of the text.
fn quote(quoted: &Quoted, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("irc-tracker-quote")
        .role(Role::Note)
        .test_support()
        .gap_1()
        .child(
            div()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(format!("{} · {}", clip(&quoted.author, 32), quoted.place)),
        )
        .child(
            div()
                .text_size(tokens.row_font)
                .text_color(tokens.fg)
                .child(rich::plain_text(
                    "irc-tracker-quote-text",
                    &clip(&quoted.text, QUOTE_CHARS),
                )),
        )
        .into_any_element()
}

/// The project or issue choice: a labelled menu.
fn picker(
    dialog: &Dialog,
    label: &'static str,
    conversation: WeakEntity<Conversation>,
    tokens: &Tokens,
) -> AnyElement {
    let shown = dialog
        .chosen
        .clone()
        .unwrap_or_else(|| "Choose…".to_owned());
    let icon = match dialog.kind {
        Kind::MakeIssue => IconName::Folder,
        Kind::RecordDecision => IconName::CircleDot,
    };
    // Neutral: the dialog's one primary action is the button that writes.
    let button = menu_button("irc-tracker-choice", icon, shown.clone(), false)
        .accessibility_label(format!("{label}: {shown}"));
    let choices = dialog.choices.clone();
    let chosen = dialog.chosen.clone();
    let menu = anchored::dropdown_menu(button, move |menu, _, _| {
        choices.iter().fold(menu, |menu, choice| {
            let conversation = conversation.clone();
            let value = choice.clone();
            menu.item(
                PopupMenuItem::new(SharedString::from(choice.clone()))
                    .checked(chosen.as_ref() == Some(choice))
                    .on_click(move |_, _, cx| {
                        let value = value.clone();
                        _ = conversation.update(cx, |this, cx| choose(this, value, cx));
                    }),
            )
        })
    });
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted)
                .child(label),
        )
        .child(menu)
        .into_any_element()
}
