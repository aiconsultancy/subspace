//! All-view decision entries; detail navigation is installed in slice 4.
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _,
};
use subspace_api_client::model::{DecisionStatus, DecisionSummary, IssueActivity};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    badge::badge,
    rich::plain_text,
};
use subspace_framework::lucide::IconName;

pub(super) fn render(
    entry: &IssueActivity,
    issues: &crate::Issues,
    tokens: &Tokens,
    timestamp: String,
    cx: &mut Context<crate::Issues>,
) -> Div {
    let decision = issues.issue_view.snapshot.as_ref().and_then(|issue| {
        entry
            .decision_id()
            .and_then(|id| issue.timeline().decision(id))
    });
    let title = summary(decision);
    let actor = crate::activity_words::actor_name(entry.activity.actor())
        .unwrap_or_else(|| "Unknown actor".into());
    let byline = format!("{actor} · {timestamp}");
    let mut heading = h_flex()
        .min_w_0()
        .flex_wrap()
        .gap_2()
        .child(decision_text(entry, "title", &title));
    if let Some(decision) = decision {
        let (label, color) = status(decision.status, tokens);
        heading = heading.child(badge(label, tokens.surface, color).test_text(label));
    }
    let mut row = v_flex()
        .id(format!("timeline-event:{}", entry.id))
        .aria_label(match decision {
            Some(decision) => format!(
                "{title}, {}, {actor}, {timestamp}",
                status(decision.status, tokens).0
            ),
            None => "Decision logged, details unavailable".into(),
        })
        .test_support()
        .min_w_0()
        .gap_2()
        .text_sm()
        .text_color(tokens.muted)
        .child(heading)
        .child(decision_text(entry, "byline", &byline));
    if let Some(decision) = decision {
        let id = decision.id.clone();
        let (label, _) = status(decision.status, tokens);
        row = row.child(
            action_button(
                format!("issue-activity-decision:{}", entry.id),
                Intent::Quiet,
                Some(IconName::Scale),
                "Open decision",
            )
            .accessibility_label(format!(
                "Open decision {}, {label}, logged by {actor}",
                decision.title
            ))
            .on_click(cx.listener(move |this, _, _, _| this.open_timeline_decision(&id))),
        );
    }
    gpui_kit::div().child(row)
}

fn decision_text(entry: &IssueActivity, part: &str, text: &str) -> impl gpui_kit::IntoElement {
    plain_text(format!("timeline-decision-{part}:{}", entry.id), text).test_text(text)
}

fn summary(decision: Option<&DecisionSummary>) -> String {
    match decision {
        Some(decision) => format!("Decision logged · {}", decision.title),
        None => "Decision logged · Earlier decision unavailable".into(),
    }
}

fn status(status: DecisionStatus, tokens: &Tokens) -> (&'static str, gpui_kit::Hsla) {
    match status {
        DecisionStatus::Accepted => ("Accepted", tokens.green),
        DecisionStatus::Rejected => ("Rejected", tokens.red),
        DecisionStatus::Proposed => ("Proposed", tokens.muted),
        DecisionStatus::Superseded => ("Superseded", tokens.muted),
        _ => ("Unknown status", tokens.muted),
    }
}

impl crate::Issues {
    /// Safe no-op until the read-only panel is delivered in slice 4.
    fn open_timeline_decision(&mut self, _id: &str) {}
}

#[cfg(test)]
mod tests {
    use super::summary;
    use serde_json::json;
    use subspace_api_client::model::DecisionSummary;

    #[test]
    fn decision_text_never_falls_back_to_the_uuid() {
        let id = "00000000-0000-0000-0000-000000000001";
        let decision: DecisionSummary = serde_json::from_value(
            json!({"id":id,"title":"Use cursor paging","status":"accepted"}),
        )
        .unwrap();
        assert_eq!(
            summary(Some(&decision)),
            "Decision logged · Use cursor paging"
        );
        assert!(!summary(Some(&decision)).contains(id));
        assert_eq!(
            summary(None),
            "Decision logged · Earlier decision unavailable"
        );
        assert!(!summary(None).contains(id));
    }
}
