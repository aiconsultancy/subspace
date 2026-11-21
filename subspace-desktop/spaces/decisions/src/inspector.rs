//! The inspector: a peek at the selected decision from the list read, with
//! Open; or, beside an open page, that decision's facts. Supersession rows
//! select or open the other decision, and linked issue rows open the issue
//! in the Space that answers for issues.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{Tokens, feedback, links, rich, truncated, workspace};
use subspace_framework::lucide::IconName;

use crate::Decisions;
use crate::open_issue;
use crate::page::{Page, PageState};
use crate::peek::{self, Evidence, Issue, Peek, Related, Relations};
use crate::words;

const INSPECTOR_ID: &str = "decisions-inspector";
const NOTHING_SELECTED: &str = "Select a decision to see it here.";

pub(crate) fn render(this: &Decisions, cx: &mut Context<Decisions>) -> workspace::Inspector {
    let tokens = Tokens::current(cx);
    let inspector = workspace::Inspector::new(INSPECTOR_ID, "Decision").on_close(
        "Close inspector",
        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_inspector(cx)),
    );
    if let Some(page) = this.shown_page() {
        return inspector.children(page_body(this, page, &tokens, cx));
    }
    match this.selected_item() {
        Some(item) => {
            let peek = peek::peek(item, &this.session.people, &this.session.projects);
            inspector
                .action(open_button(cx))
                .children(peek_body(&peek, &tokens, cx))
        }
        None => inspector.child(note(&tokens, "decisions-inspector-empty", NOTHING_SELECTED)),
    }
}

fn open_button(cx: &mut Context<Decisions>) -> AnyElement {
    action_button(
        "decisions-open",
        Intent::Neutral,
        Some(IconName::ArrowUpRight),
        "Open",
    )
    .accessibility_label("Open the decision")
    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_selected(cx)))
    .into_any_element()
}

fn note(tokens: &Tokens, id: &'static str, message: &'static str) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .aria_label(message)
        .child(feedback::empty_line(tokens, format!("{id}-text"), message))
        .into_any_element()
}

fn peek_body(peek: &Peek, tokens: &Tokens, cx: &mut Context<Decisions>) -> Vec<AnyElement> {
    let mut parts = vec![
        head(peek, tokens),
        fact_section("Properties", &peek.facts, "peek", cx),
    ];
    parts.extend(relation_parts(&peek.relations, Evidenced::Yes, tokens, cx));
    parts
}

/// Beside an open page: the decision's facts once it is read.
fn page_body(
    this: &Decisions,
    page: &Page,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Vec<AnyElement> {
    let PageState::Loaded(detail) = &page.state else {
        return vec![note(
            tokens,
            "decisions-inspector-waiting",
            "Nothing to show yet.",
        )];
    };
    let facts = peek::facts(detail, &this.session.people, &this.session.projects);
    let relations = peek::relations(detail);
    let mut parts = vec![fact_section("Properties", &facts, "page", cx)];
    // The page shows the evidence itself.
    parts.extend(relation_parts(&relations, Evidenced::No, tokens, cx));
    parts
}

/// The status in words beside its glyph, then the title.
fn head(peek: &Peek, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("decisions-peek-head")
        .test_support()
        .aria_label(format!("{}, {}", words::status(peek.status), peek.title))
        .gap_2()
        .min_w_0()
        .child(
            h_flex()
                .gap_1()
                .text_size(tokens.secondary_size())
                .text_color(tokens.tone(words::status_tone(peek.status)))
                .child(Icon::from(words::status_icon(peek.status)).xsmall())
                .child(words::status(peek.status)),
        )
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(rich::plain_text_with(
                    "decisions-peek-title",
                    &peek.title,
                    MenuChat::Off,
                )),
        )
        .into_any_element()
}

/// A titled section of labelled facts. Each fact is named "label: value"
/// for assistive technology.
fn fact_section(
    title: &'static str,
    facts: &[(&'static str, String)],
    scope: &'static str,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    workspace::section(title, cx)
        .children(facts.iter().map(|(label, value)| {
            let text = rich::plain_text_with(
                SharedString::from(format!("decisions-{scope}-fact:{label}")),
                value,
                MenuChat::Off,
            );
            fact_line(label, value, text, cx)
        }))
        .into_any_element()
}

/// Whether a part of the Space shows a decision's evidence.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Evidenced {
    Yes,
    No,
}

/// Supersedes, Superseded by, Linked issues and Evidence, each when there
/// is something in it.
fn relation_parts(
    relations: &Relations,
    evidenced: Evidenced,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Vec<AnyElement> {
    let mut parts = Vec::new();
    parts.extend(
        relations
            .supersedes
            .as_ref()
            .map(|older| related_section("Supersedes", older, cx)),
    );
    parts.extend(
        relations
            .superseded_by
            .as_ref()
            .map(|newer| related_section("Superseded by", newer, cx)),
    );
    if !relations.issues.is_empty() {
        parts.push(issues_section(
            &relations.issues,
            relations.more_issues,
            tokens,
            cx,
        ));
    }
    if evidenced == Evidenced::Yes {
        parts.extend(
            relations
                .evidence
                .as_ref()
                .map(|evidence| self::evidence(evidence, "inspector", tokens, cx)),
        );
    }
    parts
}

/// A section with one row that selects or opens the other decision.
fn related_section(
    title: &'static str,
    related: &Related,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let (id, name) = (related.id.clone(), related.title.clone());
    let button = action_button(
        SharedString::from(format!("decisions-related:{}", related.id)),
        Intent::Quiet,
        Some(IconName::ArrowLeftRight),
        "",
    )
    .min_w_0()
    .max_w_full()
    .child(truncated::text(
        SharedString::from(format!("decisions-related-title:{}", related.id)),
        related.title.clone(),
    ))
    .accessibility_label(format!("{title}: {}", related.title))
    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        this.follow(&id, &name, cx);
    }));
    workspace::section(title, cx)
        .child(
            div()
                .id(SharedString::from(format!(
                    "decisions-related-row:{}",
                    related.id
                )))
                .test_support()
                .aria_label(format!("{title}: {}", related.title))
                .min_w_0()
                .child(button),
        )
        .into_any_element()
}

/// The linked issues, each opening in the Space that answers for issues.
fn issues_section(
    issues: &[Issue],
    more: usize,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let rows: Vec<AnyElement> = issues.iter().map(|issue| issue_row(issue, cx)).collect();
    let more = (more > 0).then(|| {
        feedback::empty_line(
            tokens,
            "decisions-more-issues",
            format!("and {more} more linked issues"),
        )
    });
    workspace::section("Linked issues", cx)
        .children(rows)
        .children(more)
        .into_any_element()
}

fn issue_row(issue: &Issue, cx: &mut Context<Decisions>) -> AnyElement {
    let identifier = issue.identifier.clone();
    let button = action_button(
        SharedString::from(format!("decisions-issue:{}", issue.identifier)),
        Intent::Quiet,
        Some(IconName::CircleDot),
        "",
    )
    .min_w_0()
    .max_w_full()
    .child(truncated::text(
        SharedString::from(format!("decisions-issue-title:{}", issue.identifier)),
        format!("{} {}", issue.identifier, issue.title),
    ))
    .accessibility_label(format!("Open issue {}: {}", issue.identifier, issue.title))
    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        open_issue::open(&identifier, this, cx);
    }));
    div()
        .id(SharedString::from(format!(
            "decisions-issue-row:{}",
            issue.identifier
        )))
        .test_support()
        .aria_label(format!("{} {}", issue.identifier, issue.title))
        .min_w_0()
        .child(button)
        .into_any_element()
}

/// The evidence: where it came from, its reference, a link when it is a
/// safe address, and the excerpt. The page draws the same section.
pub(crate) fn evidence(
    evidence: &Evidence,
    scope: &'static str,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let kind = evidence
        .kind
        .map(|kind| fact_line("Kind", kind, SharedString::from(kind), cx));
    let reference = evidence.reference.as_deref().map(|reference| {
        let value = match links::safe_url(reference) {
            Some(url) => links::external_marked(
                SharedString::from(format!("decisions-{scope}-evidence-link")),
                reference.to_owned(),
                url,
                tokens,
            ),
            None => SharedString::from(reference.to_owned()).into_any_element(),
        };
        fact_line("Reference", reference, value, cx)
    });
    let excerpt = evidence.excerpt.as_deref().map(|excerpt| {
        div().text_color(tokens.fg).child(rich::plain_text_with(
            SharedString::from(format!("decisions-{scope}-evidence-excerpt")),
            excerpt,
            MenuChat::Off,
        ))
    });
    workspace::section("Evidence", cx)
        .children(kind)
        .children(reference)
        .children(excerpt)
        .into_any_element()
}

/// One evidence fact, named "label: text" for assistive technology.
fn fact_line(label: &'static str, text: &str, value: impl IntoElement, cx: &App) -> AnyElement {
    workspace::fact(label, value, cx)
        .aria_label(format!("{label}: {text}"))
        .test_support()
        .into_any_element()
}
