mod decision;
mod rail;

use std::collections::{HashMap, HashSet};

use chrono::Local;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::model::{ActivityKind, Comment, IssueActivity};
use subspace_framework::dashboard::{
    Tokens,
    dates::{parse_date, stamp},
    rich::rich_text_prepared,
};

use crate::detail::timeline::entry::Entry;
use crate::{activity_words, detail::timeline::TimelineMode};

pub(super) fn render(
    visible: &[&IssueActivity],
    mode: TimelineMode,
    issues: &crate::Issues,
    tokens: &Tokens,
    cx: &mut Context<crate::Issues>,
) -> Div {
    let entries = ordered_entries(visible, issues);
    let ids: HashSet<_> = entries
        .iter()
        .filter_map(|entry| entry.comment().map(|comment| comment.id.as_str()))
        .collect();
    let mut replies = threaded_replies(&entries, &ids, mode);
    let mut group_date = String::new();
    let mut list = v_flex().min_w_0();
    for entry in entries {
        let parent = entry.parent();
        let in_thread = parent.is_some_and(|root| ids.contains(root));
        if in_thread && mode == TimelineMode::Comments {
            continue;
        }
        let date = entry_date(entry);
        if date != group_date && !date.is_empty() {
            list = list.child(rail::date(&date, tokens));
            group_date = date;
        }
        let event = as_event(entry);
        let mut body = if let Some(activity) =
            event.filter(|activity| activity.activity.kind() == ActivityKind::DecisionLogged)
        {
            decision::render(activity, issues, tokens, time(entry, &group_date), cx)
        } else {
            row(entry, issues, tokens, in_thread, time(entry, &group_date))
        };
        if let Some(thread) = entry
            .comment()
            .and_then(|comment| replies.remove(comment.id.as_str()))
        {
            body =
                body.children(thread.into_iter().map(|reply| {
                    row(reply, issues, tokens, true, time(reply, &group_date)).mt_2()
                }));
        }
        if let Some(comment) = entry.comment().filter(|c| c.parent_comment_id.is_none()) {
            body = body.child(crate::authoring::view::thread(issues, comment, tokens, cx));
        }
        list = list.child(rail::entry(entry.id(), body, event.is_none(), tokens));
    }
    list
}

fn ordered_entries<'a>(visible: &[&'a IssueActivity], issues: &'a crate::Issues) -> Vec<Entry<'a>> {
    let projected = issues
        .issue_view
        .snapshot
        .as_ref()
        .map(|i| i.timeline().projected())
        .unwrap_or_default();
    let loaded = issues
        .issue_view
        .snapshot
        .as_ref()
        .and_then(|i| i.timeline().page())
        .map_or(&[][..], |p| p.items.as_slice());
    merged_entries(visible, projected, loaded)
}

fn merged_entries<'a>(
    visible: &[&'a IssueActivity],
    projected: &'a [Comment],
    loaded: &[IssueActivity],
) -> Vec<Entry<'a>> {
    let loaded_ids: HashSet<_> = loaded
        .iter()
        .filter_map(|entry| entry.comment.as_ref().map(|c| c.id.as_str()))
        .collect();
    let mut entries: Vec<_> = visible
        .iter()
        .map(|e| Entry::Activity(e))
        .chain(
            projected
                .iter()
                .filter(|c| !loaded_ids.contains(c.id.as_str()))
                .map(Entry::Posted),
        )
        .collect();
    entries.sort_by(|a, b| {
        parse_date(a.at())
            .cmp(&parse_date(b.at()))
            .then_with(|| a.id().cmp(b.id()))
    });
    entries
}

fn threaded_replies<'a>(
    entries: &[Entry<'a>],
    ids: &HashSet<&str>,
    mode: TimelineMode,
) -> HashMap<&'a str, Vec<Entry<'a>>> {
    let mut replies: HashMap<&str, Vec<Entry<'_>>> = HashMap::new();
    if mode == TimelineMode::Comments {
        for entry in entries {
            if let Some(root) = entry.parent().filter(|root| ids.contains(root)) {
                replies.entry(root).or_default().push(*entry);
            }
        }
    }
    replies
}

fn entry_date(entry: Entry<'_>) -> String {
    parse_date(entry.at())
        .map(|instant| {
            instant
                .with_timezone(&Local)
                .format("%-d %b %Y")
                .to_string()
        })
        .unwrap_or_default()
}

fn time(entry: Entry<'_>, group_date: &str) -> String {
    if group_date.is_empty() || entry_date(entry) != group_date {
        return stamp(entry.at());
    }
    parse_date(entry.at()).map_or_else(String::new, |instant| {
        instant.with_timezone(&Local).format("%H:%M").to_string()
    })
}

fn as_event(entry: Entry<'_>) -> Option<&IssueActivity> {
    match entry {
        Entry::Activity(activity) if activity.activity.kind() != ActivityKind::CommentCreated => {
            Some(activity)
        }
        _ => None,
    }
}

fn row(
    entry: Entry<'_>,
    issues: &crate::Issues,
    tokens: &Tokens,
    nested: bool,
    timestamp: String,
) -> Div {
    let mut row = v_flex().gap_2().min_w_0();
    if nested {
        row = row.pl_4().border_l_1().border_color(tokens.hair);
    }
    if let Some(activity) = as_event(entry) {
        return row.child(event(activity, timestamp, tokens));
    }
    if entry.parent().is_some() && !nested {
        row = row.child(
            div()
                .text_sm()
                .text_color(tokens.muted)
                .child("Reply in a thread"),
        );
    }
    row.child(match entry.comment() {
        Some(comment) if comment.deleted_at.is_none() => {
            comment_row(comment, entry, issues, tokens, timestamp)
        }
        _ => div()
            .id(format!("timeline-tombstone:{}", entry.id()))
            .test_support()
            .text_color(tokens.muted)
            .child("Comment deleted")
            .into_any_element(),
    })
}

fn comment_row(
    comment: &Comment,
    entry: Entry<'_>,
    issues: &crate::Issues,
    tokens: &Tokens,
    notes: String,
) -> AnyElement {
    let byline = comment_byline(comment, entry.id(), tokens, notes);
    let mut row = v_flex()
        .id(format!("timeline-comment:{}", entry.id()))
        .test_support()
        .min_w_0()
        .gap_2()
        .child(byline);
    if let Some(body) = issues.issue_view.comment_bodies.get(&comment.id) {
        row = row.child(crate::media::view::markdown(
            rich_text_prepared(
                format!("timeline-comment-body:{}", entry.id()),
                body.clone(),
                |url, _, _| {
                    subspace_framework::dashboard::links::safe_url(url).is_none()
                        || url.starts_with("subspace-search:")
                },
            ),
            issues,
        ));
    }
    if let Some(provenance) = &comment.provenance {
        row = row.child(div().text_sm().text_color(tokens.muted).child(format!(
            "Imported from {} by {}",
            provenance.source_system, comment.actual_author.display_name
        )));
    }
    row.into_any_element()
}

fn comment_byline(comment: &Comment, id: &str, tokens: &Tokens, mut notes: String) -> Div {
    let author = comment
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.original_author_display_name.as_deref())
        .filter(|name| !name.is_empty())
        .unwrap_or(&comment.actual_author.display_name);
    if comment.edited_at.is_some() {
        if !notes.is_empty() {
            notes.push_str(" · ");
        }
        notes.push_str("Edited");
    }
    h_flex()
        .items_baseline()
        .flex_wrap()
        .gap_2()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(subspace_framework::dashboard::rich::plain_text(
                    format!("timeline-author:{id}"),
                    author,
                )),
        )
        .child(
            div()
                .id(format!("timeline-time:{}", id))
                .test_support()
                .text_sm()
                .text_color(tokens.muted)
                .child(subspace_framework::dashboard::rich::plain_text(
                    format!("timeline-notes:{id}"),
                    &notes,
                )),
        )
}

fn event(entry: &IssueActivity, timestamp: String, tokens: &Tokens) -> AnyElement {
    let change = match entry.activity.kind() {
        ActivityKind::Other => "Issue activity recorded".into(),
        kind => activity_words::change(kind, entry.activity.completed_item_count()),
    };
    let actor = activity_words::actor_name(entry.activity.actor());
    let summary = actor.map_or_else(|| change.clone(), |actor| format!("{actor} · {change}"));
    h_flex()
        .id(format!("timeline-event:{}", entry.id))
        .test_support()
        .items_baseline()
        .flex_wrap()
        .gap_2()
        .text_sm()
        .text_color(tokens.muted)
        .child(subspace_framework::dashboard::rich::plain_text(
            format!("timeline-summary:{}", entry.id),
            &summary,
        ))
        .when(!timestamp.is_empty(), |event| {
            event.child(subspace_framework::dashboard::rich::plain_text(
                format!("timeline-event-time:{}", entry.id),
                &format!("· {timestamp}"),
            ))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::merged_entries;
    use subspace_api_client::model::{IssueActivity, Page};

    #[test]
    fn a_loaded_comment_replaces_its_projection_even_outside_the_visible_slice() {
        let pages: serde_json::Value =
            serde_json::from_str(include_str!("../../../../fixtures/issue-timeline.json")).unwrap();
        let page: Page<IssueActivity> = serde_json::from_value(pages["head"].clone()).unwrap();
        let projected = vec![page.items[0].comment.clone().unwrap()];
        let visible = [&page.items[0]];
        assert_eq!(merged_entries(&visible, &projected, &page.items).len(), 1);
        assert!(merged_entries(&[], &projected, &page.items).is_empty());
    }
}
