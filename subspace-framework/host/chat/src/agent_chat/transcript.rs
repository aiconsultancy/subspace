//! The transcript rows: the user's bubbles, Copilot's replies and the folded
//! thinking and tool rows, in the Copilot desktop arrangement.

use super::attachments::{Attachment, attachment_key, attachment_label, split_user_message};
use super::logic::{is_steering, is_streaming, reply_is_complete, thinking_label};
use super::{
    ChatPanel, ClipboardItem, Context, Div, Entry, Format, SharedString, Stateful, Tokens,
    ToolCall, ToolStatus, TranscriptEntry, control, div, h_flex, px, rich_text, v_flex,
};
use crate::action::{Intent, icon_button};
use crate::truncated::{self, Shortened};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Hsla, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
};
use subspace_lucide::{IconName, SvgIcon};

/// The tallest an expanded tool result grows before it scrolls.
const RESULT_HEIGHT: f32 = 220.;
/// The rule beside expanded reasoning.
const RULE_WIDTH: f32 = 2.;
/// The note above a message Copilot took into a running turn.
const STEERING_LABEL: &str = "Steering";

fn chevron(expanded: bool) -> SvgIcon {
    if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    }
}

/// A running call shows a loader; a call that never finished shows that it
/// stopped; anything else shows the tool glyph.
fn tool_icon(status: ToolStatus) -> SvgIcon {
    match status {
        ToolStatus::Running => IconName::LoaderCircle,
        ToolStatus::Aborted => IconName::CircleSlash,
        _ => IconName::SquareTerminal,
    }
}

/// A failure is red; every other state is muted chrome.
fn tool_colour(status: ToolStatus, tokens: &Tokens) -> Hsla {
    if status == ToolStatus::Failed {
        tokens.red
    } else {
        tokens.muted
    }
}

/// A call's head: its title, and the note that it never finished. The title
/// is already one line, so the row cannot grow.
fn tool_label(call: &ToolCall) -> String {
    if call.status == ToolStatus::Aborted {
        return format!("{} (did not finish)", call.title.trim());
    }
    call.title.trim().to_owned()
}

/// The result lines an open row shows; a row that is shut or reported
/// nothing shows none.
fn result_lines(call: &ToolCall, expanded: bool) -> Vec<SharedString> {
    if !expanded {
        return Vec::new();
    }
    call.result.lines().map(SharedString::from).collect()
}

impl ChatPanel {
    /// One attached reference: a folded row that opens its content.
    fn render_attachment(&self, key: String, item: &Attachment, cx: &mut Context<Self>) -> Div {
        let expanded = self.tab().expanded.contains(&key);
        let toggled = key.clone();
        let lines: Vec<SharedString> = if expanded {
            item.content.lines().map(SharedString::from).collect()
        } else {
            Vec::new()
        };
        v_flex()
            .items_end()
            .gap_1()
            .child(
                control(
                    SharedString::from(format!("ref:{key}")),
                    attachment_label(&item.label, expanded),
                )
                .icon(IconName::Paperclip)
                .accessibility_label(format!("Attached reference: {}", item.label))
                .on_click(cx.listener(move |chat, _, _, cx| {
                    chat.toggle_expanded(&toggled);
                    cx.notify();
                })),
            )
            .children(
                lines
                    .into_iter()
                    .map(|line| div().whitespace_normal().child(line)),
            )
    }

    /// The user's own message: a bubble on the right, with the references the
    /// runtime appended folded under it. A message Copilot took into a turn
    /// that was already running carries the steering label above the bubble.
    fn render_user(
        &self,
        index: usize,
        entry: &TranscriptEntry,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let split = split_user_message(entry.entry.text());
        v_flex()
            .items_end()
            .gap_1()
            .when(is_steering(&entry.entry), |row| {
                row.child(div().text_color(tokens.muted).child(STEERING_LABEL))
            })
            .child(crate::chat_bubble::bubble(tokens).child(rich_text(
                ("chat-markdown", index),
                Format::Markdown,
                &split.message,
            )))
            .children(
                split.attachments.iter().enumerate().map(|(at, item)| {
                    self.render_attachment(attachment_key(&entry.id, at), item, cx)
                }),
            )
    }

    /// The per-turn actions under a finished reply.
    fn render_reply_footer(index: usize, text: &str, cx: &mut Context<Self>) -> Div {
        let reply = text.to_owned();
        h_flex().child(
            icon_button(
                ("chat-copy", index),
                Intent::Quiet,
                IconName::Copy,
                "Copy reply",
            )
            .compact()
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(reply.clone()));
            })),
        )
    }

    /// Copilot's reply: full-width Markdown with no heading above it.
    fn render_reply(&self, index: usize, text: &str, cx: &mut Context<Self>) -> Div {
        let complete = reply_is_complete(index, self.tab().entries.len(), self.tab().is_busy());
        v_flex()
            .gap_1()
            .child(rich_text(("chat-markdown", index), Format::Markdown, text))
            .when(complete, |reply| {
                reply.child(Self::render_reply_footer(index, text, cx))
            })
    }

    /// The muted one-line head of a foldable row: a kind icon, the label and
    /// a chevron that shows the fold state.
    fn fold_head(
        id: String,
        icon: SvgIcon,
        label: String,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        h_flex().child(
            control(SharedString::from(format!("fold:{id}")), label)
                .icon(icon)
                .child(chevron(expanded))
                .on_click(cx.listener(move |chat, _, _, cx| {
                    chat.toggle_expanded(&id);
                    cx.notify();
                })),
        )
    }

    /// Reasoning: one muted row that opens italic text beside a left rule.
    fn render_thinking(
        &self,
        entry: &TranscriptEntry,
        seconds: Option<u64>,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let expanded = self.tab().expanded.contains(&entry.id);
        let streaming = is_streaming(&entry.id);
        let label = thinking_label(seconds, streaming);
        let text = entry.entry.text().to_owned();
        v_flex()
            .gap_1()
            .text_color(tokens.muted)
            .child(Self::fold_head(
                entry.id.clone(),
                IconName::CircleDashed,
                label,
                expanded,
                cx,
            ))
            .when(expanded, |row| {
                row.child(
                    div()
                        .pl_2()
                        .border_l(px(RULE_WIDTH))
                        .border_color(tokens.line)
                        .italic()
                        .whitespace_normal()
                        .child(text),
                )
            })
    }

    /// One tool call: its title, a muted one-line argument summary, and the
    /// result in a bounded block once opened. A failure is red.
    fn render_tool(
        &self,
        id: &str,
        call: &ToolCall,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let expanded = self.tab().expanded.contains(id);
        v_flex()
            .gap_1()
            .text_color(tool_colour(call.status, tokens))
            .child(
                h_flex()
                    .min_w_0()
                    .gap_1()
                    .child(Self::fold_head(
                        id.to_owned(),
                        tool_icon(call.status),
                        tool_label(call),
                        expanded,
                        cx,
                    ))
                    .child(Self::render_detail(id, call.shown_detail(), tokens, cx)),
            )
            .children(Self::tool_result(id, call, expanded, tokens, cx))
    }

    /// The argument summary: muted monospace, cut with an ellipsis and
    /// offered in full on hover.
    fn render_detail(id: &str, detail: &str, tokens: &Tokens, cx: &mut Context<Self>) -> Shortened {
        truncated::text(
            SharedString::from(format!("tool-detail:{id}")),
            detail.to_owned(),
        )
        .flex_1()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.mono_font)
        .text_color(tokens.muted)
    }

    /// The result or the error, in a bounded scrollable monospace block, once
    /// the row is open and the call reported something.
    fn tool_result(
        id: &str,
        call: &ToolCall,
        expanded: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let lines: Vec<SharedString> = result_lines(call, expanded);
        (!lines.is_empty()).then(|| {
            div()
                .id(SharedString::from(format!("result:{id}")))
                .max_h(px(RESULT_HEIGHT))
                .overflow_y_scroll()
                .pl_2()
                .border_l(px(RULE_WIDTH))
                .border_color(tokens.line)
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(tokens.mono_font)
                .children(
                    lines
                        .into_iter()
                        .map(|line| div().whitespace_normal().child(line)),
                )
        })
    }

    /// The row an entry draws, or nothing when a view toggle hides its kind.
    fn render_row(
        &self,
        index: usize,
        entry: &TranscriptEntry,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        match &entry.entry {
            Entry::User { .. } => Some(self.render_user(index, entry, tokens, cx)),
            Entry::Assistant(text) | Entry::Info(text) => Some(self.render_reply(index, text, cx)),
            Entry::Thinking { seconds, .. } => self
                .show_thinking
                .then(|| self.render_thinking(entry, *seconds, tokens, cx)),
            Entry::Tool(call) => self.canvas_tool_card(call, tokens, cx).or_else(|| {
                self.show_tools
                    .then(|| self.render_tool(&entry.id, call, tokens, cx))
            }),
        }
    }

    pub(super) fn render_transcript(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let rows = self
            .tab()
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                Some(
                    v_flex()
                        .pb_3()
                        .child(self.render_row(index, entry, tokens, cx)?),
                )
            })
            .collect::<Vec<_>>();
        div()
            .id("chat-transcript")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_2()
            .children(rows)
            .when(self.tab().entries.is_empty(), |body| {
                body.child(self.guidance())
            })
            .children(self.render_status(tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::{chevron, result_lines, tool_colour, tool_icon, tool_label};
    use crate::tokens::Tokens;
    use subspace_agent::{ToolCall, ToolStatus};
    use subspace_lucide::IconName;

    fn call(status: ToolStatus, result: &str) -> ToolCall {
        ToolCall {
            call_id: "c1".into(),
            title: "Read services".into(),
            detail: "github_search".into(),
            status,
            result: result.into(),
        }
    }

    #[test]
    fn a_running_call_shows_a_loader_and_a_failure_shows_the_error_colour() {
        assert_eq!(tool_icon(ToolStatus::Running), IconName::LoaderCircle);
        assert_eq!(tool_icon(ToolStatus::Done), IconName::SquareTerminal);
        assert_eq!(tool_icon(ToolStatus::Failed), IconName::SquareTerminal);
        let tokens = Tokens::one_dark();
        assert_eq!(tool_colour(ToolStatus::Failed, &tokens), tokens.red);
        assert_eq!(tool_colour(ToolStatus::Done, &tokens), tokens.muted);
        assert_eq!(tool_colour(ToolStatus::Running, &tokens), tokens.muted);
    }

    #[test]
    fn a_result_shows_only_while_the_row_is_open() {
        let done = call(ToolStatus::Done, "one\ntwo");
        assert_eq!(result_lines(&done, true), ["one", "two"]);
        assert!(
            result_lines(&done, false).is_empty(),
            "a shut row shows none"
        );
        assert!(
            result_lines(&call(ToolStatus::Running, ""), true).is_empty(),
            "a call that reported nothing shows no block"
        );
    }

    #[test]
    fn a_row_marks_a_call_that_never_finished() {
        let stopped = call(ToolStatus::Aborted, "");
        assert_eq!(tool_label(&stopped), "Read services (did not finish)");
        assert_eq!(tool_icon(ToolStatus::Aborted), IconName::CircleSlash);
        assert_eq!(
            tool_label(&call(ToolStatus::Done, "")),
            "Read services",
            "a finished call is named plainly"
        );
    }

    #[test]
    fn the_chevron_follows_the_fold_state() {
        assert_eq!(chevron(true), IconName::ChevronDown);
        assert_eq!(chevron(false), IconName::ChevronRight);
    }
}
