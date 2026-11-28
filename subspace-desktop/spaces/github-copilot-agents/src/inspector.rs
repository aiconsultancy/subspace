//! The inspector: the selected row, or beside the opened tab the opened
//! one. A short name, Open, the GitHub link and Close in its header; the
//! status, title, an excerpt, the facts and the pull request in its body;
//! the row's actions in its footer.

use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::{Tokens, feedback, rich, truncated, workspace};
use subspace_framework::lucide::IconName;

use crate::CopilotAgents;
use crate::keys::{
    CopyLink, OpenInChat, OpenPullRequest, OpenSession, OpenSubject, ShowLog, ToggleMonitor,
};
use crate::model::AgentRow;
use crate::peek::{self, Act, Peek};

const INSPECTOR_ID: &str = "github-copilot-agents-inspector";
/// The header's title with nothing selected.
const EMPTY_TITLE: &str = "Task";
/// The inspector's Open, which opens the selection as a tab.
pub(crate) const OPEN_ID: &str = "github-copilot-agents-open";
/// The inspector's GitHub link.
pub(crate) const GITHUB_ID: &str = "github-copilot-agents-inspector-github";

// qual:allow(coupling, oi) reason: "One file per concern of the Copilot Agents dashboard; this one owns the inspector."
impl CopilotAgents {
    /// The inspector for the target: the opened row while its tab shows,
    /// else the selected row, else a note that asks for a selection.
    pub(crate) fn inspector(&self, cx: &mut Context<Self>) -> workspace::Inspector {
        let tokens = Tokens::current(cx);
        let Some(row) = self.target() else {
            return close(workspace::Inspector::new(INSPECTOR_ID, EMPTY_TITLE), cx)
                .child(note(&tokens));
        };
        let peek = peek::peek(row, self.loaded_detail(&row.key));
        let mut inspector = workspace::Inspector::new(INSPECTOR_ID, peek.name.clone());
        if self.item.showing().is_none() {
            inspector = inspector.action(open_button(&row.key, cx));
        }
        if let Some(link) = crate::browser::link(row) {
            inspector = inspector.action(
                icon_button(GITHUB_ID, Intent::Quiet, IconName::ExternalLink, link.label).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.open_session(&OpenSession, window, cx);
                    }),
                ),
            );
        }
        let banner = self.sessions_error(&row.key, "github-copilot-agents-inspector-banner", cx);
        close(inspector, cx)
            .child(head(&peek, &tokens))
            .child(facts(&peek, cx))
            .children(peek.pull.as_ref().map(|pull| pull_section(pull, cx)))
            .children(banner)
            .footer(self.footer(row, cx))
    }

    /// The row's actions, each one a key and a palette command already.
    fn footer(&self, row: &AgentRow, cx: &mut Context<Self>) -> AnyElement {
        let shown = self.log.as_ref().is_some_and(|log| log.task_key == row.key);
        h_flex()
            .flex_wrap()
            .gap_2()
            .children(
                peek::acts(row, shown)
                    .into_iter()
                    .map(|act| act_button(act, cx)),
            )
            .into_any_element()
    }
}

/// Close, the header's last control, hides the inspector.
fn close(inspector: workspace::Inspector, cx: &mut Context<CopilotAgents>) -> workspace::Inspector {
    inspector.on_close(
        "Close inspector",
        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_inspector(cx)),
    )
}

fn note(tokens: &Tokens) -> AnyElement {
    div()
        .id("github-copilot-agents-inspector-empty")
        .test_support()
        .aria_label(peek::NOTHING_SELECTED)
        .child(feedback::empty_line(
            tokens,
            "github-copilot-agents-inspector-empty-text",
            peek::NOTHING_SELECTED,
        ))
        .into_any_element()
}

/// Open: the selection as a tab beside the list, as a double click or
/// Enter opens it.
fn open_button(key: &str, cx: &mut Context<CopilotAgents>) -> AnyElement {
    let key = key.to_owned();
    action_button(
        OPEN_ID,
        Intent::Neutral,
        Some(IconName::ArrowUpRight),
        "Open",
    )
    .accessibility_label("Open the task as a tab")
    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_item(&key, cx)))
    .into_any_element()
}

/// A status in words beside its glyph, in its tone: the words and the
/// glyph's shape tell the states apart without the colour.
pub(crate) fn status_line(status: &peek::Status, tokens: &Tokens) -> Div {
    h_flex()
        .gap_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.tone(status.tone))
        .child(Icon::from(status.icon).xsmall())
        .child(status.words.clone())
}

/// The status, the title and the excerpt.
fn head(peek: &Peek, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("github-copilot-agents-peek-head")
        .test_support()
        .aria_label(format!("{}, {}", peek.status.words, peek.title))
        .gap_2()
        .min_w_0()
        .child(status_line(&peek.status, tokens))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(rich::plain_text(
                    "github-copilot-agents-peek-title",
                    &peek.title,
                )),
        )
        .children(peek.excerpt.as_deref().map(|excerpt| {
            div()
                .text_size(tokens.secondary_size())
                .text_color(tokens.fg)
                .child(rich::plain_text(
                    "github-copilot-agents-peek-excerpt",
                    excerpt,
                ))
        }))
        .into_any_element()
}

/// The facts, each named "label: value" for assistive technology.
fn facts(peek: &Peek, cx: &mut Context<CopilotAgents>) -> AnyElement {
    workspace::section("Properties", cx)
        .children(peek.facts.iter().map(|(label, value)| {
            let text = rich::plain_text(
                SharedString::from(format!("github-copilot-agents-peek-fact:{label}")),
                value,
            );
            workspace::fact(*label, text, cx)
                .aria_label(format!("{label}: {value}"))
                .test_support()
                .into_any_element()
        }))
        .into_any_element()
}

/// The task's pull request as one row that opens it on GitHub (p).
fn pull_section(pull: &peek::Pull, cx: &mut Context<CopilotAgents>) -> AnyElement {
    let tokens = Tokens::current(cx);
    let label = pull.label();
    let button = action_button(
        "github-copilot-agents-peek-pull",
        Intent::Quiet,
        Some(pull.status.icon),
        "",
    )
    .min_w_0()
    .max_w_full()
    .child(
        div()
            .text_color(tokens.tone(pull.status.tone))
            .child(truncated::text(
                "github-copilot-agents-peek-pull-label",
                label.clone(),
            )),
    )
    .accessibility_label(format!("Open pull request {label} on GitHub"))
    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
        this.open_pull_request(&OpenPullRequest, window, cx);
    }));
    workspace::section("Pull request", cx)
        .child(button)
        .into_any_element()
}

/// One footer action, doing what its key does.
fn act_button(act: Act, cx: &mut Context<CopilotAgents>) -> AnyElement {
    let (id, label, icon) = act.words();
    action_button(id, Intent::Neutral, Some(icon), label)
        .on_click(
            cx.listener(move |this, _: &ClickEvent, window, cx| match act {
                Act::Chat => this.open_in_chat(&OpenInChat, window, cx),
                Act::Log { .. } => this.show_log(&ShowLog, window, cx),
                Act::Monitor { .. } => this.toggle_monitor(&ToggleMonitor, window, cx),
                Act::CopyLink => this.copy_link(&CopyLink, window, cx),
                Act::Subject => this.open_subject(&OpenSubject, window, cx),
            }),
        )
        .into_any_element()
}
