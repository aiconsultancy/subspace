//! The IRC Settings page: the network list, one network's form, highlight
//! words, and the agent setup actions.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::toggle::{switch, toggle_row};
use subspace_framework::dashboard::{Tokens, reveal};
use subspace_framework::lucide::IconName;

use super::IrcSettings;
use super::form::NetworkForm;
use super::validate::parse_port;

pub(super) const AGENT_NOTE: &str = "A coding agent can add networks to the IRC settings file, irc/config.json, for you. The prompt names the file on this computer, its format, and how to keep passwords private.";
const HIGHLIGHTS_NOTE: &str = "Words that mark a message as mentioning you on every network, besides your nick. Separate them with commas.";
pub(super) const HISTORY_NOTE: &str = "Channel and private messages are kept on this computer, restored when Subspace starts and searchable. To stop keeping them or delete them, open Search (Cmd/Ctrl+Shift+F): Cache Channels turns this off, and Clear cache… deletes what is kept.";
const APPLY_NOTE: &str =
    "Save applies network changes without a restart. Unchanged networks stay connected.";

impl Render for IrcSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        self.edited()
            .map(|network| super::network_view::render(self, network, &tokens, cx))
            .unwrap_or_else(|| list_page(self, &tokens, cx))
    }
}

fn list_page(editor: &IrcSettings, tokens: &Tokens, cx: &mut Context<IrcSettings>) -> AnyElement {
    v_flex()
        .id("irc-settings")
        .test_support()
        .gap_4()
        .child(
            v_flex()
                .gap_3()
                .child(fields::heading("Networks", cx))
                .child(network_list(editor, tokens, cx))
                .child(
                    h_flex().child(
                        action_button(
                            "irc-add-network",
                            Intent::Neutral,
                            Some(IconName::Plus),
                            "Add network",
                        )
                        .disabled(!editor.enabled)
                        .on_click(
                            cx.listener(|editor, _, window, cx| editor.add_network(window, cx)),
                        ),
                    ),
                )
                .child(fields::note(tokens, APPLY_NOTE)),
        )
        .child(
            fields::form_field(
                "Highlight words",
                fields::input(
                    &editor.highlights,
                    InputRole::Configuration,
                    "Highlight words",
                )
                .id("irc-highlights")
                .disabled(!editor.enabled),
            )
            .child(fields::note(tokens, HIGHLIGHTS_NOTE)),
        )
        .child(
            v_flex()
                .id("irc-history-settings")
                .test_support()
                .gap_3()
                .child(fields::heading("History", cx))
                .child(fields::note(tokens, HISTORY_NOTE)),
        )
        // Captain notices route through the tracker's initiatives and fleets.
        .children(
            subspace_api_client::has_backend().then(|| captain_notifications(editor, tokens, cx)),
        )
        .child(agent_setup(editor, tokens, cx))
        .into_any_element()
}

fn captain_notifications(
    editor: &IrcSettings,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> Div {
    v_flex()
        .gap_2()
        .child(toggle_row(
            "Notify ship captains",
            switch("irc-captain-notifications", cx)
                .checked(editor.captain_notifications)
                .disabled(!editor.enabled)
                .on_click(cx.listener(|editor, on: &bool, _, cx| {
                    editor.captain_notifications = *on;
                    cx.notify();
                })),
        ))
        .child(fields::note(
            tokens,
            "Post to the ship’s initiative channel when its PR merges or its deployment starts, fails or succeeds.",
        ))
}

fn network_list(editor: &IrcSettings, tokens: &Tokens, cx: &mut Context<IrcSettings>) -> Div {
    if editor.networks.is_empty() {
        return fields::note(tokens, "No networks yet.");
    }
    v_flex().gap_3().children(
        editor
            .networks
            .iter()
            .map(|network| network_row(editor, network, tokens, cx)),
    )
}

fn network_row(
    editor: &IrcSettings,
    network: &NetworkForm,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    let key = network.key;
    let title = network.title(cx);
    let problem = editor.problems(key, cx).into_iter().next();
    let removing = editor.removing == Some(key);
    v_flex()
        .id(("irc-network", key))
        .test_support()
        .min_w_0()
        .gap_2()
        .pb_3()
        .border_b_1()
        .border_color(tokens.line)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.clone()))
        .child(fields::note(tokens, summary(network, cx)))
        .children(problem.map(|problem| {
            fields::field_error(tokens, ("irc-network-problem", key), &problem.message)
        }))
        .child(if removing {
            removal(&title, key, cx).into_any_element()
        } else {
            row_actions(key, editor.enabled, cx).into_any_element()
        })
        .into_any_element()
}

/// `host:port · TLS · nick`, from the draft.
fn summary(network: &NetworkForm, cx: &App) -> String {
    let text = network.text(cx);
    let port = parse_port(&text.port).map_or(text.port.clone(), |port| port.to_string());
    let security = if text.tls { "TLS" } else { "Plaintext" };
    let mut parts = vec![
        format!("{}:{port}", text.host.trim()),
        security.to_owned(),
        text.nickname.trim().to_owned(),
    ];
    if !text.auto_connect {
        parts.push("Connects on request".to_owned());
    }
    parts.retain(|part| !part.is_empty() && part != ":");
    parts.join(" · ")
}

fn row_actions(key: u64, enabled: bool, cx: &mut Context<IrcSettings>) -> Div {
    h_flex()
        .gap_2()
        .child(
            action_button(
                ("irc-edit-network", key),
                Intent::Neutral,
                Some(IconName::Pencil),
                "Edit…",
            )
            .disabled(!enabled)
            .on_click(cx.listener(move |editor, _, _, cx| editor.open(key, cx))),
        )
        .child(
            action_button(
                ("irc-remove-network", key),
                Intent::Quiet,
                Some(IconName::Trash),
                "Remove…",
            )
            .disabled(!enabled)
            .on_click(cx.listener(move |editor, _, _, cx| editor.ask_remove(Some(key), cx))),
        )
}

fn removal(title: &str, key: u64, cx: &mut Context<IrcSettings>) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .id(("irc-remove-confirmation", key))
        .test_support()
        .gap_2()
        .child(removal_title(title))
        .child(fields::note(
            &tokens,
            "Save disconnects it and removes it and its saved passwords from irc/config.json.",
        ))
        .child(
            h_flex()
                .gap_2()
                .child(
                    action_button(
                        ("irc-remove-cancel", key),
                        Intent::Neutral,
                        Some(IconName::X),
                        "Cancel",
                    )
                    .on_click(cx.listener(|editor, _, _, cx| editor.ask_remove(None, cx))),
                )
                .child(
                    action_button(
                        ("irc-remove-confirm", key),
                        Intent::Reject,
                        Some(IconName::Trash),
                        "Remove",
                    )
                    .on_click(cx.listener(move |editor, _, _, cx| editor.remove(key, cx))),
                ),
        )
}

pub(super) fn removal_title(title: &str) -> String {
    format!("Remove “{title}”?")
}

fn agent_setup(editor: &IrcSettings, tokens: &Tokens, cx: &App) -> Div {
    let path = editor.settings_file.clone();
    v_flex()
        .gap_3()
        .child(fields::heading("Set up with a coding agent", cx))
        .child(fields::note(tokens, AGENT_NOTE))
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(crate::setup::copy_prompt_button("irc-settings-copy-prompt"))
                .children(path.map(|path| reveal::button("irc-settings-reveal-config", path))),
        )
}
