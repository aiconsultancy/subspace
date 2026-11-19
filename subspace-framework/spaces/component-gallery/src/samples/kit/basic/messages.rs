//! GPUI Kit messaging and people basics: attachments, avatars, bubbles,
//! markers, messages, the message scroller and the questionnaire.
//!
//! The message samples follow GPUI Kit's `message_story.rs` and
//! `marker_story.rs` compositions (Apache-2.0), cut down to fixture samples.

use gpui_kit::component::attachment::{
    Attachment, AttachmentContent, AttachmentDescription, AttachmentMedia, AttachmentStatus,
    AttachmentTitle,
};
use gpui_kit::component::avatar::{Avatar, AvatarGroup};
use gpui_kit::component::bubble::{Bubble, BubbleVariant};
use gpui_kit::component::marker::{Marker, MarkerContent, MarkerVariant};
use gpui_kit::component::message::{
    Message, MessageAlignment, MessageContent, MessageFooter, MessageHeader,
};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::questionnaire::{
    Questionnaire, QuestionnaireActions, QuestionnaireChoice, QuestionnaireChoiceDefinition,
    QuestionnaireChoices, QuestionnaireItem, QuestionnaireItemDefinition, QuestionnaireNext,
    QuestionnairePrevious, QuestionnaireProgress, QuestionnaireState, QuestionnaireTitle,
};
use gpui_kit::component::{Icon, Sizable as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _,
    Styled as _, Window, div,
};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, sample, sized};

const WIDE: f32 = 360.;
const SCROLLER: (f32, f32) = (360., 200.);
const UPLOAD_PERCENT: f32 = 45.;
const PEOPLE: [&str; 4] = ["Ada Example", "Sam Fixture", "Lee Sample", "Kim Demo"];
const GROUP_LIMIT: usize = 3;
/// The fixture conversation of the message scroller.
const LINES: [(&str, &str); 8] = [
    ("Ada Example", "Is the fixture ready?"),
    (
        "Sam Fixture",
        "Almost. The gallery shows every component now.",
    ),
    ("Ada Example", "Does the scroller follow new messages?"),
    ("Sam Fixture", "Yes, while you stay at the end."),
    ("Ada Example", "And when I scroll up?"),
    (
        "Sam Fixture",
        "It keeps your place and shows a jump button.",
    ),
    ("Ada Example", "Great."),
    ("Sam Fixture", "Scroll up to try it."),
];
const STEPS: [(&str, &str, [(&str, &str); 2]); 2] = [
    (
        "theme",
        "Which theme do you use?",
        [("dark", "Dark"), ("light", "Light")],
    ),
    (
        "density",
        "Which density do you prefer?",
        [("compact", "Compact"), ("comfortable", "Comfortable")],
    ),
];

/// The retained scroller of the `MessageScroller` sample.
pub(crate) fn scroller_state(cx: &mut App) -> Entity<MessageScrollerState> {
    cx.new(|cx| MessageScrollerState::new(LINES.len(), cx))
}

/// The retained state of the `Questionnaire` sample.
pub(crate) fn questionnaire_state(cx: &mut App) -> Entity<QuestionnaireState> {
    let items: Vec<_> = STEPS
        .iter()
        .map(|(name, label, choices)| {
            QuestionnaireItemDefinition::new(*name, *label).with_choices(
                choices.map(|(value, label)| QuestionnaireChoiceDefinition::new(value, label)),
            )
        })
        .collect();
    cx.new(|cx| fixture_questionnaire(items, cx))
}

/// GPUI Kit rejects duplicate names; the fixture has none, and an empty
/// questionnaire, which is always valid, stands in if that ever changes.
fn fixture_questionnaire(
    items: Vec<QuestionnaireItemDefinition>,
    cx: &mut Context<QuestionnaireState>,
) -> QuestionnaireState {
    match QuestionnaireState::new(items, cx) {
        Ok(state) => state,
        Err(_) => fixture_questionnaire(Vec::new(), cx),
    }
}

pub(crate) fn attachments(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let file = |id: &'static str, status: AttachmentStatus, description: &'static str| {
        Attachment::new()
            .id(id)
            .status(status)
            .media(AttachmentMedia::new().child(Icon::from(IconName::FileText)))
            .content(
                AttachmentContent::new()
                    .title(AttachmentTitle::new("fixture-report.pdf"))
                    .description(AttachmentDescription::new(description)),
            )
    };
    vec![
        sample(
            "Complete",
            file(
                "gallery-kit:attachment-complete",
                AttachmentStatus::Complete,
                "24 KB",
            ),
        ),
        sample(
            "Uploading",
            file(
                "gallery-kit:attachment-uploading",
                AttachmentStatus::Uploading,
                "Uploading…",
            )
            .progress(UPLOAD_PERCENT),
        ),
        sample(
            "Failed",
            file(
                "gallery-kit:attachment-failed",
                AttachmentStatus::Failed,
                "Upload failed",
            ),
        ),
    ]
}

pub(crate) fn avatars(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("Initials", Avatar::new().name(PEOPLE[0])),
        sample("Placeholder", Avatar::new().placeholder(IconName::User)),
        sample("Small", Avatar::new().name(PEOPLE[1]).small()),
        sample(
            "Group with a limit",
            AvatarGroup::new()
                .limit(GROUP_LIMIT)
                .ellipsis()
                .children(PEOPLE.map(|name| Avatar::new().name(name))),
        ),
    ]
}

pub(crate) fn bubbles(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("Primary", Bubble::new().child("A message I sent.")),
        sample(
            "Secondary",
            Bubble::new()
                .with_variant(BubbleVariant::Secondary)
                .child("A message I received."),
        ),
    ]
}

pub(crate) fn markers(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let marker = |variant, text: &'static str| {
        sized(
            WIDE,
            Marker::new()
                .with_variant(variant)
                .content(MarkerContent::new().child(text)),
        )
    };
    vec![
        sample(
            "Plain",
            marker(MarkerVariant::default(), "Ada Example joined"),
        ),
        sample(
            "Separator",
            marker(MarkerVariant::Separator, "Earlier messages"),
        ),
        sample("Border", marker(MarkerVariant::Border, "Unread messages")),
        sample(
            "Loading",
            sized(
                WIDE,
                Marker::new()
                    .loading(true)
                    .content(MarkerContent::new().child("Thinking…")),
            ),
        ),
    ]
}

fn message(name: &'static str, text: &'static str, alignment: MessageAlignment) -> Message {
    let variant = match alignment {
        MessageAlignment::End => BubbleVariant::default(),
        _ => BubbleVariant::Secondary,
    };
    Message::new()
        .alignment(alignment)
        .avatar(Avatar::new().name(name).small())
        .header(MessageHeader::new().child(name))
        .content(MessageContent::new().bubble(Bubble::new().with_variant(variant).child(text)))
}

pub(crate) fn messages(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Received and sent",
        sized(
            WIDE,
            v_flex()
                .gap_2()
                .child(message(
                    PEOPLE[0],
                    "Can you review the fixture?",
                    MessageAlignment::Start,
                ))
                .child(
                    message(PEOPLE[1], "Sure, after lunch.", MessageAlignment::End)
                        .footer(MessageFooter::new().child("Delivered")),
                ),
        ),
    )]
}

fn scroller_row(index: usize) -> AnyElement {
    let Some((name, text)) = LINES.get(index) else {
        return div().into_any_element();
    };
    let alignment = if index.is_multiple_of(2) {
        MessageAlignment::Start
    } else {
        MessageAlignment::End
    };
    message(name, text, alignment).into_any_element()
}

pub(crate) fn message_scrollers(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = gallery.retained.kit.basic.scroller.clone();
    vec![sample(
        "Follows the end; scroll up for the jump button",
        bounded(
            SCROLLER,
            tokens,
            MessageScroller::new("gallery-kit:message-scroller", state, |index, _, _| {
                scroller_row(index)
            })
            .size_full(),
        ),
    )]
}

pub(crate) fn questionnaires(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.basic.questionnaire;
    let mut questionnaire = Questionnaire::new(state).child(QuestionnaireProgress::new(state));
    for (name, _, choices) in STEPS {
        let mut options = QuestionnaireChoices::new(state, name);
        for (value, _) in choices {
            options = options.child(QuestionnaireChoice::new(state, name, value));
        }
        questionnaire = questionnaire.child(
            QuestionnaireItem::new(state, name)
                .child(QuestionnaireTitle::new(state, name))
                .child(options),
        );
    }
    let actions = QuestionnaireActions::new(state)
        .child(QuestionnairePrevious::new(state))
        .child(QuestionnaireNext::new(state));
    vec![sample(
        "Two questions; choose and go next",
        sized(WIDE, questionnaire.child(actions)),
    )]
}
