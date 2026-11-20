use crate::{Issues, scope};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, base::TestTextExt as _, div, prelude::FluentBuilder as _,
};
use subspace_api_client::{
    hub::ReadError,
    model::{AgentRank, DecisionEvidenceKind, DecisionListItem, DecisionMakerType, DecisionStatus},
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    badge::badge,
    feedback,
};
use subspace_framework::lucide::IconName;

pub(crate) fn status(value: DecisionStatus) -> &'static str {
    match value {
        DecisionStatus::Proposed => "Proposed",
        DecisionStatus::Accepted => "Accepted",
        DecisionStatus::Rejected => "Rejected",
        DecisionStatus::Superseded => "Superseded",
        _ => "Unknown status",
    }
}

pub(crate) fn status_badge(
    value: DecisionStatus,
    tokens: &Tokens,
) -> impl gpui_kit::IntoElement + use<> {
    let color = match value {
        DecisionStatus::Accepted => tokens.green,
        DecisionStatus::Rejected => tokens.red,
        _ => tokens.muted,
    };
    badge(status(value), tokens.surface, color)
        .when(value == DecisionStatus::Superseded, |badge| {
            badge.border_1().border_color(tokens.line)
        })
        .test_text(status(value))
}

pub(crate) fn date(item: &DecisionListItem, spoken: bool) -> String {
    match (item.decided_on(), spoken) {
        (Some(date), true) => date.format("decided %-d %B %Y").to_string(),
        (Some(date), false) => date.format("Decided %-d %b %Y").to_string(),
        (None, true) => "no decided-on date".into(),
        (None, false) => "No decided-on date".into(),
    }
}

pub(crate) fn maker_short(item: &DecisionListItem) -> String {
    match item.made_by_type {
        DecisionMakerType::Agent => item.made_by_agent_name.as_deref().map_or_else(
            || "Made by an agent".into(),
            |name| format!("{name} (agent)"),
        ),
        DecisionMakerType::Human => "Made by a person".into(),
        _ => "Maker not recorded".into(),
    }
}

pub(crate) fn maker(item: &DecisionListItem) -> String {
    match item.made_by_type {
        DecisionMakerType::Human => return "A person".into(),
        DecisionMakerType::Agent => (),
        _ => return "Maker not recorded".into(),
    }
    let rank = match item.made_by_agent_rank {
        Some(AgentRank::RearAdmiral) => Some("rear admiral"),
        Some(AgentRank::Captain) => Some("captain"),
        Some(AgentRank::Crew) => Some("crew"),
        _ => None,
    };
    let name = item.made_by_agent_name.as_deref().unwrap_or("An agent");
    [Some(name), rank, item.made_by_agent_model.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(crate) fn evidence(kind: DecisionEvidenceKind) -> &'static str {
    match kind {
        DecisionEvidenceKind::IrcMessage => "IRC message",
        DecisionEvidenceKind::TeamsMessage => "Teams message",
        DecisionEvidenceKind::SubspaceComment => "Subspace comment",
        DecisionEvidenceKind::DocumentOrUrl => "Document or URL",
        DecisionEvidenceKind::Meeting => "Meeting",
        _ => "Other evidence",
    }
}

pub(crate) fn failure(error: &ReadError, fallback: &'static str) -> String {
    match error {
        ReadError::Network => scope::network_message(),
        ReadError::SignInNeeded => scope::SIGN_IN_MESSAGE.to_owned(),
        _ => fallback.to_owned(),
    }
}

pub(crate) fn offline(tokens: &Tokens, id: &'static str) -> Div {
    notice(tokens, id, "Offline").child(feedback::empty_line(
        tokens,
        format!("{id}-hint"),
        scope::network_message(),
    ))
}

pub(crate) fn notice(
    tokens: &Tokens,
    id: &'static str,
    message: impl Into<gpui_kit::SharedString>,
) -> Div {
    let message = message.into();
    div().child(
        div()
            .id(id)
            .aria_label(message.clone())
            .test_support()
            .child(subspace_framework::dashboard::feedback::empty_line(
                tokens,
                format!("{id}-text"),
                message,
            )),
    )
}

pub(crate) fn retry(
    id: &'static str,
    label: &'static str,
    action: impl Fn(&mut Issues, &mut Context<Issues>) + 'static,
    cx: &mut Context<Issues>,
) -> impl gpui_kit::IntoElement {
    action_button(id, Intent::Neutral, Some(IconName::RotateCw), "Retry")
        .accessibility_label(label)
        .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
}
