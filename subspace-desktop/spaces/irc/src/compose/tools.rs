//! The IRC-specific actions in the compact message input.

use gpui_kit::component::{Disableable as _, h_flex, menu::PopupMenuItem};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, WeakEntity, Window,
};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::{ChatInput, anchored};
use subspace_framework::lucide::IconName;

use crate::conversation::Conversation;

type Channel = (WeakEntity<Conversation>, bool);

/// The shared input owns its tools view. The tools keep only weak references.
pub(super) fn install(input: &Entity<ChatInput>, channel: Option<Channel>, cx: &mut App) {
    let tools = cx.new(|cx| {
        let observe = channel
            .as_ref()
            .and_then(|(owner, _)| owner.upgrade())
            .map(|owner| cx.observe(&owner, |_, _, cx| cx.notify()));
        Tools {
            input: input.downgrade(),
            channel,
            _observe: observe,
        }
    });
    input.update(cx, |input, cx| input.set_addon(Some(tools.into()), cx));
}

struct Tools {
    input: WeakEntity<ChatInput>,
    channel: Option<Channel>,
    _observe: Option<Subscription>,
}

impl Render for Tools {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.input.clone();
        h_flex()
            .flex_shrink_0()
            .gap_1()
            .children(
                self.channel
                    .as_ref()
                    .map(|(owner, _)| commands(owner.clone())),
            )
            .children(self.channel.as_ref().and_then(|(owner, attaches)| {
                (*attaches && subspace_api_client::capabilities().chat_uploads)
                    .then(|| attach(owner.clone(), cx))
            }))
            .child(
                icon_button(
                    "irc-mention",
                    Intent::Quiet,
                    IconName::AtSign,
                    "Mention someone",
                )
                .compact()
                .on_click(move |_, window, cx| {
                    _ = input.update(cx, |input, cx| mention(input, window, cx));
                }),
            )
            .child(format_menu(self.input.clone()))
    }
}

fn commands(owner: WeakEntity<Conversation>) -> impl IntoElement {
    action_button(
        "irc-commands",
        Intent::Quiet,
        Some(IconName::Slash),
        "Commands",
    )
    .on_click(move |_, window, cx| {
        _ = owner.update(cx, |view, cx| super::offer::commands(view, window, cx));
    })
}

fn attach(owner: WeakEntity<Conversation>, cx: &App) -> impl IntoElement {
    let idle = owner
        .upgrade()
        .is_some_and(|view| view.read(cx).tray.idle());
    icon_button(
        "irc-attach-file",
        Intent::Quiet,
        IconName::Paperclip,
        "Attach files…",
    )
    .compact()
    .disabled(!idle)
    .on_click(move |_, _, cx| {
        _ = owner.update(cx, crate::uploads::outgoing::pick);
    })
}

fn mention(input: &mut ChatInput, window: &mut Window, cx: &mut Context<ChatInput>) {
    let range = input.selection(cx);
    let text = input.text(cx);
    let prefix = &text[..range.start];
    let marker = if prefix.ends_with(|ch: char| !ch.is_whitespace()) {
        " @"
    } else {
        "@"
    };
    input.replace_range(range, marker, window, cx);
    input.focus(window, cx);
}

fn format_menu(input: WeakEntity<ChatInput>) -> impl IntoElement {
    let button = action_button("irc-format", Intent::Quiet, None, "Aa")
        .accessibility_label("Format message");
    anchored::dropdown_menu(button, move |menu, _, _| {
        [
            (Format::Bold, "Bold"),
            (Format::Code, "Inline code"),
            (Format::Quote, "Quote"),
        ]
        .into_iter()
        .fold(menu, |menu, (format, label)| {
            let input = input.clone();
            menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                _ = input.update(cx, |input, cx| format.apply(input, window, cx));
            }))
        })
    })
}

#[derive(Clone, Copy)]
enum Format {
    Bold,
    Code,
    Quote,
}

impl Format {
    fn apply(self, input: &mut ChatInput, window: &mut Window, cx: &mut Context<ChatInput>) {
        let text = input.text(cx);
        let range = input.selection(cx);
        let (range, insert, caret) = match self {
            Self::Bold => wrapped(&text, range, "**"),
            Self::Code => wrapped(&text, range, "`"),
            Self::Quote => quote(&text, range),
        };
        input.replace_range(range, &insert, window, cx);
        if let Some(caret) = caret {
            input.set_selection(caret..caret, cx);
        }
        input.focus(window, cx);
    }
}

type Edit = (std::ops::Range<usize>, String, Option<usize>);

fn wrapped(text: &str, range: std::ops::Range<usize>, marker: &str) -> Edit {
    let insert = format!("{marker}{}{marker}", &text[range.clone()]);
    let caret = range.is_empty().then_some(range.start + marker.len());
    (range, insert, caret)
}

fn quote(text: &str, range: std::ops::Range<usize>) -> Edit {
    let start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let insert = text[start..range.end]
        .split('\n')
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    (start..range.end, insert, None)
}
