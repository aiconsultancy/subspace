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

fn human_names(item: &DecisionListItem) -> Option<String> {
    let names: Option<Vec<_>> = item
        .human_makers
        .people()
        .iter()
        .map(|maker| {
            let name = maker.display_name.trim();
            (maker.available && maker.kind == "human" && !name.is_empty()).then_some(name)
        })
        .collect();
    names
        .filter(|names| !names.is_empty())
        .map(|names| names.join(", "))
}

pub(crate) fn maker_short(item: &DecisionListItem) -> String {
    match item.made_by_type {
        DecisionMakerType::Agent => item.made_by_agent_name.as_deref().map_or_else(
            || "Made by an agent".into(),
            |name| format!("{name} (agent)"),
        ),
        DecisionMakerType::Human => human_names(item).map_or_else(
            || "Made by a person".into(),
            |name| format!("Made by {name}"),
        ),
        _ => "Maker not recorded".into(),
    }
}

pub(crate) fn maker(item: &DecisionListItem) -> String {
    match item.made_by_type {
        DecisionMakerType::Human => return human_names(item).unwrap_or_else(|| "A person".into()),
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
        ReadError::SignInNeeded => scope::sign_in_message().to_owned(),
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

#[cfg(test)]
mod tests {
    use super::{DecisionListItem, maker, maker_short};
    use serde_json::json;

    #[test]
    fn human_maker_labels_use_only_an_available_name() {
        let mut decision = json!({
            "id":"fixture","title":"Review","status":"proposed","decidedOn":"2026-11-10",
            "projectId":"project","projectKey":"SUB","issues":{"items":[],"count":0},
            "tags":[],"madeByType":"human","recordedByUserId":"recorder","revisionNumber":1,
            "createdAt":"2026-11-18T10:00:00Z","updatedAt":"2026-11-18T10:00:00Z"
        });
        for name in [None, Some(" "), Some("Ada Fixture")] {
            if let Some(name) = name {
                decision["madeBy"] = json!({
                    "userId":"maker","displayName":name,"available":true,"kind":"human"
                });
            }
            let item: DecisionListItem = serde_json::from_value(decision.clone()).unwrap();
            let expected = name.filter(|name| !name.trim().is_empty());
            assert_eq!(maker(&item), expected.unwrap_or("A person"));
            assert_eq!(
                maker_short(&item),
                format!("Made by {}", expected.unwrap_or("a person"))
            );
        }
        let ada = decision["madeBy"].clone();
        let grace = json!({"userId":"maker-2","displayName":"Grace Fixture","available":true,"kind":"human"});
        decision["madeBy"] = json!([ada, grace]);
        let joint: DecisionListItem = serde_json::from_value(decision.clone()).unwrap();
        assert_eq!(maker(&joint), "Ada Fixture, Grace Fixture");
        assert_eq!(maker_short(&joint), "Made by Ada Fixture, Grace Fixture");
        let mut without_summary = decision.clone();
        without_summary["madeBy"] = serde_json::Value::Null;
        let absent: DecisionListItem = serde_json::from_value(without_summary).unwrap();
        assert_eq!(maker(&absent), "A person");
        decision["madeBy"][1]["available"] = json!(false);
        let item: DecisionListItem = serde_json::from_value(decision).unwrap();
        assert_eq!(maker(&item), "A person");
        assert_eq!(maker_short(&item), "Made by a person");
    }
}
