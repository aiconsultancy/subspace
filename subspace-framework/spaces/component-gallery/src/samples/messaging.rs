//! Messaging: the chat inputs, the completion list, the Markdown composer,
//! chat bubbles and messages, the sender and member avatars, and the agent
//! and external avatars and crew meter beside them.

use gpui_kit::component::message::{MessageAlignment, MessageContent, MessageHeader};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, ParentElement as _, Styled as _, Subscription, Window,
    div,
};
use subspace_dashboard::chat_input::DraftLimits;
use subspace_dashboard::chat_input::completion::{CompletionItem, CompletionList};
use subspace_dashboard::composer::Composer;
use subspace_dashboard::{ChatInput, ChatInputEvent, Tokens, avatar, chat_bubble};

use super::{Sample, bounded, sample, sized};

mod identity;
use crate::ComponentGallery;
use crate::catalog::Item;
pub(crate) use identity::{agents, crew_meters, externals};

const CHAT_INPUT: &str = "board-kit/dashboard/src/chat_input.rs";
const CHAT_BUBBLE: &str = "board-kit/dashboard/src/chat_bubble.rs";
const AVATAR: &str = "board-kit/dashboard/src/avatar.rs";
const INPUT_BOX: (f32, f32) = (420., 150.);
const GROWING_BOX: (f32, f32) = (420., 340.);
const DRAFT_LINES: usize = 100;
const DRAFT_BYTES: usize = 4096;
const GROWING_DRAFT: &str = "A draft grows with each line.\n\
The send hint fades as you type.\n\
Clear the draft to bring it back.";
const COMPOSER_BOX: (f32, f32) = (420., 200.);
const LIST_WIDTH: f32 = 320.;
const MESSAGE_WIDTH: f32 = 360.;
const MESSAGE_ATTACHMENT_BYTES: usize = 25_000_000;
const PEOPLE: [&str; 2] = ["Ada Example", "Sam Fixture"];

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "chat_input::ChatInput",
        source: CHAT_INPUT,
        used_in: "The host chat panel's agent input and IRC's plain-text message input.",
        samples: chat_inputs,
        kit: None,
    },
    Item {
        name: "chat_input::completion::CompletionList",
        source: "board-kit/dashboard/src/chat_input/completion.rs",
        used_in: "Slash commands in the agent chat and @member, /command and #channel choices in IRC.",
        samples: completion_lists,
        kit: None,
    },
    Item {
        name: "composer::Composer",
        source: "board-kit/dashboard/src/composer.rs",
        used_in: "Markdown comments and reviews in the PRs and Issues boards.",
        samples: composers,
        kit: None,
    },
    Item {
        name: "chat_bubble::bubble",
        source: CHAT_BUBBLE,
        used_in: "Copilot chat transcript bubbles.",
        samples: bubbles,
        kit: None,
    },
    Item {
        name: "chat_bubble::message",
        source: CHAT_BUBBLE,
        used_in: "IRC transcript rows: a native message with a stable identity.",
        samples: messages,
        kit: None,
    },
    Item {
        name: "avatar::sender",
        source: AVATAR,
        used_in: "The sender column of IRC transcripts.",
        samples: senders,
        kit: None,
    },
    Item {
        name: "avatar::member",
        source: AVATAR,
        used_in: "IRC member lists.",
        samples: members,
        kit: None,
    },
    Item {
        name: "avatar::agent",
        source: "board-kit/dashboard/src/avatar/agent.rs",
        used_in: "Captains and the rear admiral in Channels: the transcript gutter, member lists and the channel heading.",
        samples: agents,
        kit: None,
    },
    Item {
        name: "avatar::external",
        source: "board-kit/dashboard/src/avatar/external.rs",
        used_in: "A person relayed from outside the conversation; no Space uses it yet.",
        samples: externals,
        kit: None,
    },
    Item {
        name: "avatar::crew_meter",
        source: "board-kit/dashboard/src/avatar/crew.rs",
        used_in: "How many of an agent's crew are busy; its `activity_dot` is Channels' ship and presence dot.",
        samples: crew_meters,
        kit: None,
    },
];

/// The retained entities of the messaging samples.
pub(crate) struct Messaging {
    pub(crate) agent: Entity<ChatInput>,
    pub(crate) message: Entity<ChatInput>,
    pub(crate) growing: Entity<ChatInput>,
    pub(crate) completion: Entity<CompletionList>,
    pub(crate) composer: Entity<Composer>,
    _reply_submit: Subscription,
}

impl Messaging {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let (message, reply_submit) = compact_reply(window, cx);
        let growing = cx.new(|cx| {
            let mut input = ChatInput::message(window, cx);
            input.set_plain_text(cx);
            input.set_limits(Some(DraftLimits::new(DRAFT_LINES, DRAFT_BYTES)), cx);
            input.set_text(GROWING_DRAFT, window, cx);
            input
        });
        let completion = cx.new(|cx| {
            let mut list = CompletionList::new("gallery-sample:completion", window, cx);
            list.show(completion_items(), cx);
            list
        });
        Self {
            agent: cx.new(|cx| ChatInput::new("Copilot", window, cx)),
            message,
            growing,
            completion,
            composer: cx.new(|cx| Composer::new("Comment", "Leave a fixture comment", window, cx)),
            _reply_submit: reply_submit,
        }
    }
}

fn compact_reply(window: &mut Window, cx: &mut App) -> (Entity<ChatInput>, Subscription) {
    let input = cx.new(|cx| {
        let mut input = ChatInput::compact_message("Reply", window, cx);
        input.set_attachment_budget(
            subspace_dashboard::chat_input::attachments::AttachmentBudget::new(
                1,
                MESSAGE_ATTACHMENT_BYTES,
            )
            .with_image_limit(MESSAGE_ATTACHMENT_BYTES),
            cx,
        );
        input.set_placeholder("Write a reply…", window, cx);
        input.set_limits(Some(DraftLimits::new(DRAFT_LINES, DRAFT_BYTES)), cx);
        input
    });
    let handle = window.window_handle();
    let submit = cx.subscribe(&input, move |input, event, cx| {
        if matches!(event, ChatInputEvent::Submit(_)) {
            _ = handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| input.clear(window, cx));
            });
        }
    });
    (input, submit)
}

fn completion_items() -> Vec<CompletionItem> {
    vec![
        CompletionItem::command("/join", "#channel", "Join a channel"),
        CompletionItem::person(PEOPLE[0], "@", false),
        CompletionItem::person(PEOPLE[1], "", true),
        CompletionItem::new("#fixtures"),
    ]
}

pub(crate) fn chat_inputs(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let messaging = &gallery.retained.messaging;
    vec![
        sample(
            "Agent: Markdown with Write and Preview",
            bounded(INPUT_BOX, tokens, messaging.agent.clone()),
        ),
        sample(
            "Compact message: type a reply and press Return",
            bounded(INPUT_BOX, tokens, messaging.message.clone()),
        ),
        sample(
            "Growing draft: faded hint and opt-in counter; edit or clear to try",
            bounded(GROWING_BOX, tokens, messaging.growing.clone()),
        ),
    ]
}

pub(crate) fn completion_lists(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Command, people and a channel",
        sized(LIST_WIDTH, gallery.retained.messaging.completion.clone()),
    )]
}

pub(crate) fn composers(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Write and Preview with a submit action",
        bounded(
            COMPOSER_BOX,
            tokens,
            gallery.retained.messaging.composer.clone(),
        ),
    )]
}

pub(crate) fn bubbles(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Transcript bubble",
        sized(
            MESSAGE_WIDTH,
            chat_bubble::bubble(tokens).child("A fixture reply inside the shared bubble."),
        ),
    )]
}

pub(crate) fn messages(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let row = |id: &'static str, name: &'static str, text: &'static str| {
        chat_bubble::message(
            id,
            MessageAlignment::Start,
            MessageContent::new().child(text),
        )
        .avatar(avatar::sender(name, tokens))
        .header(MessageHeader::new().child(name))
    };
    vec![sample(
        "Sender, header and text",
        sized(
            MESSAGE_WIDTH,
            v_flex()
                .gap_2()
                .child(row(
                    "gallery-sample:message-ada",
                    PEOPLE[0],
                    "Is the fixture ready?",
                ))
                .child(row(
                    "gallery-sample:message-sam",
                    PEOPLE[1],
                    "Yes, open the gallery.",
                )),
        ),
    )]
}

pub(crate) fn senders(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Initials in a rounded square",
        div().child(avatar::sender(PEOPLE[0], tokens)),
    )]
}

pub(crate) fn members(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Extra small, control radius",
        div().child(avatar::member(PEOPLE[1], tokens)),
    )]
}
