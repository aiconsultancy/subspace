//! The MCP page (REQ-01 to REQ-03, REQ-24 to REQ-28). Switches and fields
//! edit the draft until Save; Create token and Copy agent prompt act at once.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, Hsla, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, div,
};
use subspace_agent_tools::Access;
use subspace_core::config::McpFieldError;
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::fields::{FieldLayout, LabelledInput, labelled_input};
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Toast, Tokens, badge, links, rich};
use subspace_lucide::IconName;
use subspace_mcp::{GroupTool, ToolGroup};

use super::{Settings, heading, labelled, note, page_frame};
use crate::mcp::ServerState;
use crate::settings::mcp_draft::{
    BIND_FIELD, PORT_FIELD, TOKEN_FIELD, field_message, page_groups, token_action_label,
    token_state_text,
};

const TOKEN_NOTE: &str =
    "Save applies a new token to a running server. Settings never shows the token.";
const PROMPT_NOTE: &str = "Copies setup steps for an agent. The prompt names the token file but never contains the token.";
const BOARDS_NOTE: &str = "Turned-off boards serve no tools.";

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let body = v_flex()
        .gap_4()
        .child(server_section(settings, tokens, cx))
        .child(token_section(settings, tokens, cx))
        .child(prompt_section(tokens, cx))
        .child(groups_section(settings, tokens, cx));
    page_frame(
        "Serve this app's agent tools to MCP clients over HTTP.",
        body,
        tokens,
        cx,
    )
    .into_any_element()
}

fn server_section(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let error = settings.mcp.error;
    v_flex()
        .gap_3()
        .child(status_line(&crate::mcp::state(cx), tokens))
        .child(toggle_row(
            "Serve MCP over HTTP",
            subspace_dashboard::toggle::switch("mcp-enabled", cx)
                .checked(settings.mcp.enabled)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.mcp.enabled = *enabled;
                    cx.notify();
                })),
        ))
        .child(labelled_input(
            LabelledInput::new(&settings.mcp.bind, "Bind address", FieldLayout::Stacked)
                .error(field_error(error, BIND_FIELD)),
            tokens,
        ))
        .child(labelled_input(
            LabelledInput::new(&settings.mcp.port, "Port", FieldLayout::Stacked)
                .error(field_error(error, PORT_FIELD)),
            tokens,
        ))
        .child(note(
            tokens,
            "The address and port are also the defaults of the mcp --http command.",
        ))
        .child(toggle_row(
            "Require a token",
            subspace_dashboard::toggle::switch("mcp-require-token", cx)
                .checked(settings.mcp.require_token)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.mcp.require_token = *enabled;
                    cx.notify();
                })),
        ))
        .children(
            field_error(error, TOKEN_FIELD)
                .map(|message| error_text("mcp-token-error", message, tokens)),
        )
}

fn field_error(error: Option<McpFieldError>, field: &str) -> Option<String> {
    error
        .filter(|error| error.field == field)
        .map(|error| field_message(&error))
}

fn error_text(id: &'static str, message: String, tokens: &Tokens) -> Div {
    div()
        .text_color(tokens.red)
        .child(rich::plain_text(id, &message))
}

/// The state is in the text; the colour only repeats it (REQ-03).
fn status_line(state: &ServerState, tokens: &Tokens) -> Div {
    let color = match state {
        ServerState::Off => tokens.muted,
        ServerState::Listening(_) => tokens.green,
        ServerState::Error(_) => tokens.red,
    };
    labelled(
        "Server",
        div().text_color(color).child(rich::plain_text(
            "mcp-status",
            &crate::mcp::status_text(state),
        )),
    )
}

fn token_section(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let status = settings.mcp.token;
    let path = crate::mcp::token_file(cx).display().to_string();
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .gap_2()
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child("Token file")
                        .child(note(tokens, path))
                        .child(note(tokens, token_state_text(status))),
                )
                .child(
                    action_button(
                        "mcp-create-token",
                        Intent::Neutral,
                        Some(IconName::KeyRound),
                        token_action_label(status),
                    )
                    .on_click(cx.listener(|settings, _, _, cx| settings.create_mcp_token(cx))),
                ),
        )
        .child(note(tokens, TOKEN_NOTE))
}

fn prompt_section(tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    v_flex()
        .gap_1()
        .items_start()
        .child(
            action_button(
                "mcp-copy-agent-prompt",
                Intent::Neutral,
                Some(IconName::Copy),
                "Copy agent prompt",
            )
            .on_click(cx.listener(|_, _, _, cx| copy_agent_prompt(cx))),
        )
        .child(note(tokens, PROMPT_NOTE))
}

fn groups_section(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let binary = cx
        .global::<subspace_dashboard::CurrentProduct>()
        .0
        .product
        .binary_name
        .as_str()
        .to_owned();
    let groups = page_groups(&crate::mcp::groups(cx), |id| settings.mcp.group_enabled(id));
    v_flex()
        .gap_3()
        .child(heading("Tool groups", cx))
        .child(note(tokens, BOARDS_NOTE))
        .child(note(tokens, restart_note(&binary)))
        .children(groups.iter().map(|group| group_block(group, tokens, cx)))
}

fn restart_note(binary: &str) -> String {
    format!("Running {binary} mcp processes use new switches after a restart.")
}

fn group_block(group: &ToolGroup, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let id = group.id;
    let mono = cx.theme().mono_font_family.clone();
    v_flex()
        .gap_1()
        .child(toggle_row(
            group.title,
            subspace_dashboard::toggle::switch(
                ElementId::Name(format!("mcp-group-{id}").into()),
                cx,
            )
            .checked(group.served)
            .on_click(cx.listener(move |settings, enabled: &bool, _, cx| {
                settings.mcp.set_group(id, *enabled);
                cx.notify();
            })),
        ))
        .child(
            v_flex().gap_1().pl(tokens.indent).children(
                group
                    .tools
                    .iter()
                    .map(|tool| tool_row(tool, tokens, mono.clone())),
            ),
        )
}

/// A tool's name, access and actions. A switched-off group stays listed
/// with its tools marked as not served (REQ-24, REQ-25).
fn tool_row(tool: &GroupTool, tokens: &Tokens, mono: SharedString) -> Div {
    let (access, background, foreground) = access_badge(tool.access, tokens);
    let served = tool.served;
    let text = if served { tokens.fg } else { tokens.muted };
    h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .font_family(mono)
                .text_color(text)
                .child(rich::plain_text(
                    format!("mcp-tool-{}", tool.name),
                    tool.name,
                )),
        )
        .child(badge::badge(access, background, foreground))
        .when(!served, |row| {
            row.child(badge::badge("Not served", tokens.hair, tokens.muted))
        })
        .when(!tool.actions.is_empty(), |row| {
            row.child(div().text_color(tokens.muted).child(rich::plain_text(
                format!("mcp-tool-actions-{}", tool.name),
                &tool.actions.join(", "),
            )))
        })
}

fn access_badge(access: Access, tokens: &Tokens) -> (&'static str, Hsla, Hsla) {
    match access {
        Access::Read => ("Read", tokens.hair, tokens.fg),
        Access::Write => ("Write", tokens.orange, tokens.bg),
    }
}

/// Puts the agent prompt on the clipboard and confirms it (REQ-28).
fn copy_agent_prompt(cx: &mut App) {
    let toast = match crate::mcp::agent_prompt(cx) {
        Ok(prompt) => {
            links::copy(prompt, cx);
            Toast::info("Agent prompt copied")
        }
        Err(error) => Toast::error(error),
    };
    crate::mcp::push_toast(toast, cx);
}

impl Settings {
    /// Writes a new token file at once (REQ-08); the value is never shown.
    fn create_mcp_token(&mut self, cx: &mut Context<Self>) {
        match crate::mcp::create_token(cx) {
            Ok(()) => self.mcp.token = crate::mcp::token_status(&crate::mcp::token_file(cx)),
            Err(error) => self.error = Some(error.into()),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use subspace_agent_tools::Access;
    use subspace_dashboard::Tokens;

    use super::{access_badge, restart_note};

    #[test]
    fn notes_and_badges_name_the_product_and_access() {
        assert_eq!(
            restart_note("subspace"),
            "Running subspace mcp processes use new switches after a restart."
        );
        let tokens = Tokens::one_dark();
        assert_eq!(access_badge(Access::Read, &tokens).0, "Read");
        assert_eq!(access_badge(Access::Write, &tokens).0, "Write");
    }
}
