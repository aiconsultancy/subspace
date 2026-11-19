use super::super::panel::{Fact, fact};
use super::reveal;
use crate::{
    Issues,
    decision::{self, Entry, words},
};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_api_client::model::{DecisionDetail, DecisionSummary};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    dates, feedback, links, truncated,
};
use subspace_framework::lucide::IconName;

pub(super) fn section(tokens: &Tokens) -> Div {
    v_flex()
        .min_w_0()
        .border_t_1()
        .border_color(tokens.line)
        .pt_3()
        .gap_2()
}

pub(super) fn heading(title: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(title.into())
}

pub(super) fn summary(detail: &DecisionDetail, tokens: &Tokens) -> Div {
    let project = detail.project.as_ref().map_or_else(
        || detail.project_key.clone(),
        |project| {
            let archived = if project.archived { " (archived)" } else { "" };
            format!("{} · {}{archived}", detail.project_key, project.name)
        },
    );
    v_flex()
        .gap_2()
        .min_w_0()
        .child(heading(detail.title.clone(), tokens).text_size(tokens.heading_font))
        .child(
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(words::status_badge(detail.status, tokens))
                .child(feedback::empty_line(tokens, "decision-project", project)),
        )
        .child(feedback::empty_line(
            tokens,
            "decision-decided-on",
            words::date(detail, false),
        ))
        .child(fact(
            Fact::new(
                "Tags",
                if detail.tags.is_empty() {
                    "Not recorded".into()
                } else {
                    detail.tags.join(" · ")
                },
            ),
            "decision",
            tokens,
        ))
}

pub(super) fn provenance(detail: &DecisionDetail, scroll: &ScrollHandle, tokens: &Tokens) -> Div {
    let recorder = &detail.recorded_by;
    let agent = if recorder.kind == "agent" {
        " (agent)"
    } else {
        ""
    };
    let inactive = if recorder.available {
        ""
    } else {
        " (inactive)"
    };
    section(tokens)
        .child(fact(
            Fact::new("Made by", words::maker(detail)),
            "decision",
            tokens,
        ))
        .child(evidence(detail, scroll, tokens))
        .child(fact(
            Fact::new(
                "Recorded",
                format!(
                    "{}{agent}{inactive} · {}",
                    recorder.display_name,
                    dates::stamp(&detail.created_at)
                ),
            ),
            "decision",
            tokens,
        ))
        .children(
            [
                ("Accepted", &detail.accepted_at),
                ("Rejected", &detail.rejected_at),
            ]
            .into_iter()
            .filter_map(|(label, date)| {
                date.as_deref()
                    .map(|date| fact(Fact::new(label, dates::stamp(date)), "decision", tokens))
            }),
        )
}

fn evidence(detail: &DecisionDetail, scroll: &ScrollHandle, tokens: &Tokens) -> Div {
    let label = detail
        .evidence_kind
        .map(words::evidence)
        .unwrap_or("Not recorded");
    v_flex()
        .min_w_0()
        .gap_1()
        .child(fact(Fact::new("Evidence", label), "decision", tokens))
        .children(
            detail
                .evidence_reference
                .as_deref()
                .map(|value| evidence_reference(value, scroll, tokens)),
        )
        .children(
            detail.evidence_excerpt.as_deref().map(|text| {
                feedback::empty_line(tokens, "decision-evidence-excerpt", text.to_owned())
            }),
        )
}

fn evidence_reference(value: &str, scroll: &ScrollHandle, tokens: &Tokens) -> AnyElement {
    const ID: &str = "decision-evidence-reference";
    let Some(safe) = links::safe_url(value) else {
        return feedback::empty_line(tokens, ID, value.to_owned()).into_any_element();
    };
    let parsed = url::Url::parse(&safe).ok();
    // mailto is safe under the shared policy but has no URL host.
    let host = parsed
        .as_ref()
        .and_then(url::Url::host_str)
        .unwrap_or("Email link");
    let link = links::external_marked_with_label(
        ID,
        host.to_owned(),
        format!("Open evidence link: {host}"),
        safe,
        tokens,
    );
    reveal::scope(ID, scroll, link).into_any_element()
}

pub(super) fn related(
    detail: &DecisionDetail,
    scroll: &ScrollHandle,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<Div> {
    let older = relation(
        (detail.supersedes_id.as_deref(), detail.supersedes.as_ref()),
        Direction::Earlier,
        scroll,
        tokens,
        cx,
    );
    let newer = relation(
        (
            detail.superseded_by_id.as_deref(),
            detail.superseded_by.as_ref(),
        ),
        Direction::Newer,
        scroll,
        tokens,
        cx,
    );
    let linked = &detail.issues;
    if older.is_none() && newer.is_none() && linked.items.is_empty() {
        return None;
    }
    Some(
        section(tokens)
            .children(older)
            .children(newer)
            .when(!linked.items.is_empty(), |body| {
                body.child(heading(format!("Linked issues ({})", linked.count), tokens))
                    .children(linked.items.iter().map(|issue| {
                        let id = issue.identifier.clone();
                        let text = format!("{id} {}", issue.title);
                        let key = format!("decision-issue:{id}");
                        let button = action_button(
                            key.clone(),
                            Intent::Quiet,
                            Some(IconName::CircleDot),
                            "",
                        )
                        .min_w_0()
                        .max_w_full()
                        .child(truncated::text(format!("decision-issue-title:{id}"), text))
                        .accessibility_label(format!("Open issue {id}: {}", issue.title))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if !this.inline_navigation_blocked(cx) {
                                    this.focus_list(window, cx);
                                    this.open_detail(id.clone(), cx);
                                }
                            },
                        ));
                        reveal::scope(key, scroll, button)
                    }))
            }),
    )
}

#[derive(Clone, Copy)]
enum Direction {
    Earlier,
    Newer,
}

impl Direction {
    fn labels(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Earlier => ("Supersedes", "decision-supersedes", "superseded", "earlier"),
            Self::Newer => ("Superseded by", "decision-superseded-by", "newer", "newer"),
        }
    }
}

fn relation(
    (id, target): (Option<&str>, Option<&DecisionSummary>),
    direction: Direction,
    scroll: &ScrollHandle,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<Div> {
    let (title, prefix, action, temporal) = direction.labels();
    let child: AnyElement = match target {
        Some(target) => {
            let id = target.id.clone();
            let name = target.title.clone();
            let status = words::status(target.status);
            let key = format!("{prefix}:{id}");
            let button = action_button(key.clone(), Intent::Quiet, Some(IconName::Scale), "")
                .min_w_0()
                .max_w_full()
                .child(truncated::text(
                    format!("{prefix}-title:{id}"),
                    format!("{name} · {status}"),
                ))
                .accessibility_label(format!("Open {action} decision {name}, {status}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    decision::follow_link(
                        this,
                        Entry {
                            id: id.clone(),
                            title: name.clone(),
                        },
                        window,
                        cx,
                    );
                }));
            reveal::scope(key, scroll, button).into_any_element()
        }
        None => {
            id?;
            let label = match direction {
                Direction::Earlier => "Earlier decision no longer available",
                Direction::Newer => "Newer decision no longer available",
            };
            div()
                .id(format!("{prefix}-stale"))
                .aria_label(label)
                .test_support()
                .text_color(tokens.muted)
                .child(format!("The {temporal} decision is no longer available."))
                .into_any_element()
        }
    };
    Some(
        v_flex()
            .min_w_0()
            .gap_1()
            .child(heading(title, tokens))
            .child(child),
    )
}
