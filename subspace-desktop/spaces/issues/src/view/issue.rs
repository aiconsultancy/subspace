//! The full-width issue page. The top row stays available while the issue
//! scrolls, including during loads and errors.

mod decision_panel;
mod header;
mod header_progress;
mod more;
mod notices;
mod page;
mod paging;
pub(crate) mod panel;
mod properties;
mod pull_requests;
mod record;
mod relations;
mod subissues;
mod summary;
mod timeline;
mod tracker;
mod workflow;

use std::time::Instant;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus, ReadError};
use subspace_api_client::model::IssueDetail;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::rich::rich_text_prepared;
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::lucide::IconName;

pub(crate) use panel::section_heading;

use super::states;

const TWO_COLUMNS_MIN_WIDTH: f32 = 1000.;
use crate::Issues;
use crate::scope::Screen;

pub(super) fn render(
    issues: &Issues,
    open: &IssueSnapshot,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> AnyElement {
    v_flex()
        .id("issues-detail")
        .test_support()
        .flex_1()
        .min_h_0()
        .child(panel::behind_decision(
            div()
                .id("issues-detail-path")
                .child(header::path(issues, open.detail(), tokens, cx)),
            issues.decision.is_some(),
        ))
        .child(
            gpui_kit::component::h_flex()
                .items_stretch()
                .flex_1()
                .min_h_0()
                .child(panel::behind_decision(
                    v_flex()
                        .id("issues-detail-scroll")
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&issues.issue_view.scroll)
                        .child(subspace_framework::dashboard::find_in_page::scroll(
                            issues.issue_view.scroll.clone(),
                            body(issues, open, tokens, cx),
                        )),
                    issues.decision.is_some(),
                ))
                .children(
                    crate::media::text::panel(issues, tokens, window, cx)
                        .or_else(|| decision_panel::render(issues, tokens, cx)),
                ),
        )
        .into_any_element()
}

fn body(
    issues: &Issues,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    if let Some(detail) = open.detail() {
        return content(issues, detail, open, tokens, cx).into_any_element();
    }
    let identifier = open.identifier();
    match open.status() {
        IssueStatus::Loading => loading(identifier, tokens),
        IssueStatus::Error(reason) => {
            let screen = Screen::Failed(crate::sync::failure(reason));
            states::render_screen(issues, &screen, Some(identifier), tokens, cx)
        }
        IssueStatus::Unavailable | IssueStatus::Loaded => unavailable(identifier, tokens),
    }
}

fn loading(identifier: &str, tokens: &Tokens) -> AnyElement {
    let label = format!("Loading {identifier}…");
    v_flex()
        .id("issues-detail-loading")
        .aria_label(label.clone())
        .test_support()
        .child(
            div()
                .px(tokens.pad_x)
                .pt_4()
                .text_color(tokens.muted)
                .child(label)
                .child("The first load can take up to a minute while Subspace API wakes."),
        )
        .child(feedback::skeleton(tokens))
        .into_any_element()
}

fn unavailable(identifier: &str, tokens: &Tokens) -> AnyElement {
    let words = Words {
        chat: MenuChat::Off,
        ..Words::new(format!("{identifier} isn't available"), "")
    };
    div()
        .id("issues-detail-unavailable")
        .aria_label(words.title.clone())
        .test_support()
        .child(feedback::empty_words(
            tokens,
            "issue-unavailable",
            words,
            std::iter::empty::<AnyElement>(),
        ))
        .into_any_element()
}

fn content(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .when_some(
            open.updated_at()
                .filter(|_| *open.status() == IssueStatus::Error(ReadError::Network)),
            |body, read_at| {
                let age = crate::listing::updated(read_at, Instant::now());
                let words = Words {
                    chat: MenuChat::Off,
                    ..Words::new(
                        "Offline",
                        format!(
                            "{} Showing this issue as last read, {age}.",
                            crate::scope::NETWORK_MESSAGE
                        ),
                    )
                };
                body.child(feedback::error_banner_words(
                    tokens,
                    "issue-offline",
                    words,
                    [feedback::retry_action(
                        "issue-offline-retry",
                        cx.listener(|this, _, _, cx| this.reload_detail(cx)),
                    )],
                ))
            },
        )
        .child(
            page::band().child(
                page::column()
                    .pt_6()
                    .pb_12()
                    .gap_8()
                    .child(heading(issues, detail, tokens, cx))
                    .child(notices::render(detail))
                    .child(columns(issues, detail, open, tokens, cx)),
            ),
        )
}

fn heading(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .gap_5()
        .child(header::render(detail, tokens, cx))
        .children(workflow::render(issues, detail, tokens, cx))
        .when(detail.fields.kind == "tracker", |col| {
            col.child(tracker::render(issues, tokens, cx))
        })
}

fn columns(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let stacked = stacked(issues, tokens);
    gpui_kit::component::h_flex()
        .items_start()
        .gap_10()
        .when(stacked, |row| row.flex_col().gap_8())
        .child(
            v_flex()
                .id("issue-main-column")
                .test_support()
                .flex_1()
                .min_w_0()
                .when(stacked, |col| col.w_full())
                .gap_8()
                .child(summary::section(issues, detail, tokens, cx))
                .child(description(issues, detail, tokens, cx))
                .children(crate::media::view::panel(issues, open, cx))
                .children(crate::media::files::panel(open, cx))
                .children(subissues::render(detail, open, tokens, cx))
                .child(relations::render(detail, open, tokens, cx))
                .child(pull_requests::render(open, tokens, cx))
                .child(timeline::render(issues, open, tokens, cx)),
        )
        .child(
            v_flex()
                .id("issue-side-column")
                .test_support()
                .w_80()
                .min_w_0()
                .tab_group()
                .flex_shrink_0()
                .gap_8()
                .when(stacked, |col| col.w_full())
                .child(properties::render(detail, issues, tokens, cx))
                .child(record::render(detail, tokens)),
        )
}

/// Measure the page, not the adjacent viewer.
fn stacked(issues: &Issues, tokens: &Tokens) -> bool {
    issues.width - panel::side_width(issues, tokens) < TWO_COLUMNS_MIN_WIDTH
}

fn description(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    div().min_w_0().child(
        panel::section("Description", IconName::FileText, false, tokens)
            .when(issues.can_edit_issue(), |section| {
                section.title(
                    gpui_kit::component::h_flex()
                        .justify_between()
                        .child(section_heading("Description", IconName::FileText, tokens))
                        .child(
                            subspace_framework::dashboard::action::icon_button(
                                "issue-description-edit",
                                subspace_framework::dashboard::action::Intent::Quiet,
                                IconName::Pencil,
                                "Edit description…",
                            )
                            .compact()
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.edit_issue_field(
                                        crate::editing::EditTarget::Description,
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        ),
                )
            })
            .child(match &issues.issue_view.description {
                Some(text) => crate::media::view::markdown(
                    rich_text_prepared(
                        format!("issue-description:{}", detail.fields.id),
                        text.clone(),
                        |url, _, _| {
                            subspace_framework::dashboard::links::safe_url(url).is_none()
                                || url.starts_with("subspace-search:")
                        },
                    ),
                    issues,
                )
                .into_any_element(),
                None => panel::empty("No description", tokens).into_any_element(),
            }),
    )
}

fn section_title(label: impl Into<gpui_kit::SharedString>, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.heading_font)
        .text_color(tokens.fg_strong)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.into())
}
