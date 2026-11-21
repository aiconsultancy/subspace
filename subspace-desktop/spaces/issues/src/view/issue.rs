//! The issue page in the workspace: the list's view and the open issue in
//! the tab strip, Back and the path in the toolbar, the issue in one reading
//! column, and its Properties, Relations and Record in the inspector. With
//! the inspector hidden those three end the reading column instead. The
//! toolbar stays available while the issue scrolls, including during loads
//! and errors.

mod body;
mod decision_panel;
mod description;
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
mod todos;
mod tracker;
mod workflow;

use std::time::Instant;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, rems,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus, ReadError};
use subspace_api_client::model::IssueDetail;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::tabs::{self, TabSpec};
use subspace_framework::dashboard::workspace::{
    self, Inspector, Pane, READING_WIDTH_PX, Workspace,
};
use subspace_framework::lucide::IconName;

/// The one gap between the page's sections: the design guide's 24 px tier,
/// in rem so it follows the interface size.
const SECTION_GAP_REM: f32 = 1.5;

pub(crate) use panel::section_heading;
pub(crate) use record::format_date;

use crate::Issues;
use crate::docks::Dock;
use crate::navigator::View;

pub(super) fn render(
    issues: &Issues,
    open: &IssueSnapshot,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let modal = issues.decision.is_some();
    let shown = issues.shown_docks(tokens);
    let strip = div().id("issue-tabs").child(open_tabs(issues, open, cx));
    let path =
        div()
            .id("issues-detail-path")
            .child(header::toolbar(issues, open.detail(), tokens, cx));
    let pane = Pane::new("issues-pane")
        .tabs(panel::behind_decision(strip, modal).test_support())
        .toolbar(panel::behind_decision(path, modal))
        .child(page_content(issues, open, tokens, window, cx));
    Workspace::new("issues-workspace")
        .when(shown.navigator, |workspace| {
            let navigator = div()
                .id("issues-navigator")
                .size_full()
                .child(super::shell::navigator(issues, cx));
            workspace.navigator(panel::behind_decision(navigator, modal))
        })
        .pane(pane)
        .when_some(
            open.detail().filter(|_| shown.inspector),
            |workspace, detail| {
                let inspector = div()
                    .id("issues-inspector")
                    .size_full()
                    .child(inspector(issues, detail, open, tokens, cx));
                workspace.inspector(panel::behind_decision(inspector, modal))
            },
        )
        .into_any_element()
}

/// The tab strip: the list's view, which goes back to the list, and the
/// open issue, selected, whose Close goes back as Back does.
fn open_tabs(issues: &Issues, open: &IssueSnapshot, cx: &mut Context<Issues>) -> impl IntoElement {
    let view = View::current(issues.tab, issues.activity.visible);
    let title = header::tab_title(open.detail().map(|detail| detail.fields.title.as_str()));
    let list = tabs::tab(TabSpec::new(view.label(), view.icon()))
        .on_click(cx.listener(|this, _, window, cx| this.back_to_list(window, cx)));
    let key = open.identifier().to_owned();
    // The key, then the title once it has loaded.
    let spec = match loaded_title(title) {
        Some(title) => TabSpec::new(title, IconName::CircleDot).key(key),
        None => TabSpec::new(key, IconName::CircleDot),
    };
    let issue =
        tabs::tab(spec).on_close(cx.listener(|this, _, window, cx| this.go_back(window, cx)));
    tabs::pane_tab_bar("issue-tab-bar")
        .selected_index(1)
        .child(list)
        .child(issue)
}

/// The open issue's tab title once it has loaded, or `None` while the tab
/// shows only the key.
fn loaded_title(title: String) -> Option<String> {
    (!title.is_empty()).then_some(title)
}

/// The pane's content: the reading column, scrolling, with an open
/// attachment or decision panel beside it.
fn page_content(
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
        .w_full()
        .min_h_0()
        .child(
            h_flex()
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
                            body::render(issues, open, tokens, cx),
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

/// The inspector on the issue page: Properties, Relations and Record.
fn inspector(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Inspector {
    Inspector::new("issue-inspector", "Properties")
        .on_close(
            "Close inspector",
            cx.listener(|this, _, _, cx| this.toggle_dock(Dock::Inspector, cx)),
        )
        .child(side_column(issues, detail, open, tokens, cx))
}

/// Properties, Relations and Record: the inspector's body, or the end of
/// the reading column while the inspector is hidden.
fn side_column(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    // In the inspector its title names Properties; at the end of the
    // reading column a caption does.
    let docked = issues.shown_docks(tokens).inspector;
    let properties = properties::render(detail, issues, tokens, cx);
    let properties = if docked {
        properties
    } else {
        workspace::section("Properties", cx)
            .child(properties)
            .into_any_element()
    };
    v_flex()
        .id("issue-side-column")
        .test_support()
        .w_full()
        .min_w_0()
        .tab_group()
        .gap(rems(SECTION_GAP_REM))
        .child(properties)
        .child(relations::render(detail, open, tokens, cx))
        .child(record::render(detail, tokens, cx))
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
                            crate::scope::network_message()
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
                page::column(tokens)
                    .pt_6()
                    .pb_6()
                    .gap(rems(SECTION_GAP_REM))
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
        .gap(rems(SECTION_GAP_REM))
        .child(header::render(detail, tokens, cx))
        .children(workflow::render(issues, detail, tokens, cx))
        .when(detail.fields.kind == "tracker", |col| {
            col.child(tracker::render(issues, tokens, cx))
        })
}

/// The body under the heading: the main sections, then Properties,
/// Relations and Record while the inspector does not show them.
fn columns(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let docked = issues.shown_docks(tokens).inspector;
    let main = main_column(issues, detail, open, tokens, cx);
    let column = v_flex()
        .w_full()
        .min_w_0()
        .gap(rems(SECTION_GAP_REM))
        .child(main);
    if docked {
        column
    } else {
        column.child(side_column(issues, detail, open, tokens, cx))
    }
}

fn main_column(
    issues: &Issues,
    detail: &IssueDetail,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    v_flex()
        .id("issue-main-column")
        .test_support()
        .w_full()
        .min_w_0()
        .gap(rems(SECTION_GAP_REM))
        .when(subspace_api_client::capabilities().issue_summaries, |col| {
            col.child(summary::section(issues, detail, tokens, cx))
        })
        .child(description::render(issues, detail, tokens, cx))
        .children(crate::media::view::panel(issues, open, cx))
        .children(crate::media::files::panel(issues, open, cx))
        .children(subissues::render(detail, open, tokens, cx))
        .children(todos::render(open, tokens, cx))
        .child(pull_requests::render(open, tokens, cx))
        .child(timeline::render(issues, open, tokens, cx))
        .into_any_element()
}

/// Whether the page is narrow: the pane beside the docks, less an open
/// attachment or decision panel, is narrower than the reading width.
fn stacked(issues: &Issues, tokens: &Tokens) -> bool {
    issues.pane_width(tokens) < READING_WIDTH_PX
}

/// A section's title: semibold text at body size.
fn section_title(label: impl Into<gpui_kit::SharedString>, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.row_font)
        .text_color(tokens.fg_strong)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.into())
}
