//! The page's reading column: a banner when the decision is superseded or
//! supersedes another, the title and status, then the decision's sections
//! and its evidence. No wider than the reading width.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_api_client::model::DecisionDetail;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{Tokens, feedback, rich, workspace};
use subspace_framework::lucide::IconName;

use super::{Page, PageState};
use crate::Decisions;
use crate::inspector;
use crate::load::Failure;
use crate::peek::{self, Related};
use crate::words;

const GONE: &str = "This decision is no longer available. It may have been removed, or you may no longer have access.";

/// The page in the pane, scrolling on its own.
pub(crate) fn render(
    board: &Decisions,
    page: &Page,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let body = match &page.state {
        PageState::Loading => vec![loading(tokens)],
        PageState::Gone => vec![notice(tokens, "decisions-page-gone", GONE)],
        PageState::Failed(failure) => vec![failed(*failure, tokens, cx)],
        PageState::Loaded(detail) => loaded(board, detail, tokens, cx),
    };
    v_flex()
        .id("decisions-page")
        .test_support()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            v_flex()
                .w_full()
                .max_w(px(workspace::READING_WIDTH_PX))
                .px(workspace::text_edge(tokens))
                .py_4()
                .gap_6()
                .children(body),
        )
        .into_any_element()
}

fn loaded(
    board: &Decisions,
    detail: &DecisionDetail,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Vec<AnyElement> {
    let mut content = Vec::new();
    if crate::author::available() {
        content.push(super::controls::render(board, detail, cx));
    }
    content.extend(crate::history::render(&board.history, tokens, cx));
    if board.history.snapshot().is_none() {
        content.extend(reading(detail, tokens, cx));
    }
    content
}

fn loading(tokens: &Tokens) -> AnyElement {
    v_flex()
        .gap_2()
        .child(feedback::loading_line(
            tokens,
            "decisions-page-loading",
            "Loading decision…",
        ))
        .child(feedback::skeleton(tokens))
        .into_any_element()
}

fn notice(tokens: &Tokens, id: &'static str, message: &str) -> AnyElement {
    let message = SharedString::from(message.to_owned());
    div()
        .id(id)
        .test_support()
        .aria_label(message.clone())
        .child(feedback::empty_line(tokens, format!("{id}-text"), message))
        .into_any_element()
}

fn failed(failure: Failure, tokens: &Tokens, cx: &mut Context<Decisions>) -> AnyElement {
    let retry = action_button(
        "decisions-page-retry",
        Intent::Neutral,
        Some(IconName::RotateCw),
        "Retry",
    )
    .accessibility_label("Retry the decision")
    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.read_page(cx)));
    v_flex()
        .gap_2()
        .child(notice(tokens, "decisions-page-failed", &failure.message()))
        .child(h_flex().child(retry))
        .into_any_element()
}

/// The loaded decision: banners, the heading, the sections, the evidence.
pub(crate) fn reading(
    detail: &DecisionDetail,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Vec<AnyElement> {
    let relations = peek::relations(detail);
    let mut parts = Vec::new();
    parts.extend(
        relations
            .superseded_by
            .as_ref()
            .map(|newer| banner(Banner::SupersededBy, newer, tokens, cx)),
    );
    parts.extend(
        relations
            .supersedes
            .as_ref()
            .map(|older| banner(Banner::Supersedes, older, tokens, cx)),
    );
    parts.push(heading(detail, tokens));
    parts.extend(
        peek::prose(detail)
            .into_iter()
            .map(|(title, text)| section(title, text, tokens)),
    );
    parts.extend(
        relations
            .evidence
            .as_ref()
            .map(|evidence| inspector::evidence(evidence, "page", tokens, cx)),
    );
    parts
}

/// Which way a banner points.
#[derive(Clone, Copy)]
enum Banner {
    SupersededBy,
    Supersedes,
}

impl Banner {
    fn label(self) -> &'static str {
        match self {
            Banner::SupersededBy => "Superseded by",
            Banner::Supersedes => "Supersedes",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Banner::SupersededBy => "decisions-superseded-banner",
            Banner::Supersedes => "decisions-supersedes-banner",
        }
    }
}

/// A clear line above the title that names the other decision and opens
/// it.
fn banner(
    kind: Banner,
    related: &Related,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let (id, title) = (related.id.clone(), related.title.clone());
    let open = action_button(
        SharedString::from(format!("{}-open", kind.id())),
        Intent::Neutral,
        Some(IconName::ArrowRight),
        related.title.clone(),
    )
    .accessibility_label(format!("Open {}", related.title))
    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        this.open_page(id.clone(), title.clone(), cx);
    }));
    h_flex()
        .id(kind.id())
        .test_support()
        .aria_label(format!("{} {}", kind.label(), related.title))
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px(workspace::text_edge(tokens))
        .py_2()
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(tokens.line)
        .bg(tokens.surface)
        .child(
            Icon::from(IconName::ArrowLeftRight)
                .xsmall()
                .flex_shrink_0()
                .text_color(tokens.yellow),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(kind.label()),
        )
        .child(open)
        .into_any_element()
}

/// The title, then the status in words beside its glyph and the date.
fn heading(detail: &DecisionDetail, tokens: &Tokens) -> AnyElement {
    let status = detail.status;
    v_flex()
        .id("decisions-page-heading")
        .test_support()
        .aria_label(detail.title.clone())
        .gap_2()
        .min_w_0()
        .child(
            div()
                .text_size(tokens.heading_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(rich::plain_text_with(
                    "decisions-page-title",
                    &detail.title,
                    MenuChat::Off,
                )),
        )
        .child(
            h_flex()
                .gap_2()
                .text_size(tokens.secondary_size())
                .child(
                    h_flex()
                        .gap_1()
                        .text_color(tokens.tone(words::status_tone(status)))
                        .child(Icon::from(words::status_icon(status)).xsmall())
                        .child(words::status(status)),
                )
                .child(
                    div()
                        .text_color(tokens.muted)
                        .child(format!("Decided {}", words::decided_long(detail))),
                ),
        )
        .into_any_element()
}

/// One section of the decision, its text as Markdown.
fn section(title: &'static str, text: &str, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id(SharedString::from(format!("decisions-section:{title}")))
        .test_support()
        .aria_label(title)
        .gap_2()
        .min_w_0()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(title),
        )
        .child(
            rich::rich_text(
                SharedString::from(format!("decisions-prose:{title}")),
                rich::Format::Markdown,
                text,
            )
            .chat(MenuChat::Off),
        )
        .into_any_element()
}
