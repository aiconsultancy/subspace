mod account;
mod background_updates;
mod board_fields;
mod connection;
#[cfg(debug_assertions)]
mod developer;
mod general;
mod github_credentials;
mod layout;
mod mcp;
pub(super) mod navigation;
mod pages;
#[path = "view_render.rs"]
mod render;
mod secret;
mod sources;
mod terminal;
mod updates;

use secret::{SecretRow, secret_row};

use super::Settings;
use super::copy::{help, text};
use super::form::SettingsPage;
use crate::notifications::NotificationChoice;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, Context, Div, IntoElement, ParentElement, Styled, div};
use subspace_dashboard::density::{FONT_SIZES, TAB_FONT_SIZE};
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Tokens, anchored, rich};
const DIALOG_CHROME_HEIGHT: f32 = 260.;
const MIN_BODY_HEIGHT: f32 = 40.;
fn body_height(viewport_height: f32) -> f32 {
    (viewport_height - DIALOG_CHROME_HEIGHT).max(MIN_BODY_HEIGHT)
}
impl Settings {
    fn token_field(
        &self,
        index: usize,
        locked: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let token = &self.form.tokens[index];
        secret_row(
            SecretRow {
                id: ("saved-token", index).into(),
                label: token.label.into(),
                input: &token.input,
                clear: token.clear,
                saved: self.file.token_present(token.section),
                locked,
            },
            cx.listener(move |settings, _, window, cx| {
                settings.toggle_clear(index, window, cx);
            }),
            tokens,
            self,
            cx,
        )
    }
    fn appearance(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let body = v_flex()
            .gap_4()
            .child(labelled(
                "Theme",
                anchored::select(&self.themes)
                    .accessibility_label("Theme")
                    .search_placeholder("Find a theme…"),
            ))
            .child(self.fonts_group(tokens, cx))
            .child(labelled(
                "Icon button size",
                anchored::select(&self.icon_buttons.choice)
                    .id("icon-button-size")
                    .accessibility_label("Icon button size"),
            ).child(note(tokens, "Resize icon buttons throughout the application. Text keeps its current size. Save applies this choice without a restart.")))
            .child(labelled(
                "Icons",
                anchored::select(&self.agent_icons.choice)
                    .id("agent-icons")
                    .accessibility_label("Icons"),
            ).child(note(tokens, "How fleet agents' and ships' icons show in Spaces: as drawn, small beside their names, or off. Save applies this choice without a restart.")))
            .child(note(tokens, help::APPEARANCE))
            .child(note(tokens, theme_directory_note(cx)));
        page_frame(
            "Preview the theme and fonts. Save keeps appearance choices and applies the icon button size.",
            body,
            tokens,
            cx,
        )
        .into_any_element()
    }

    /// One row per role: the font select flexes and the size input stays
    /// compact on the right.
    fn fonts_group(&self, tokens: &Tokens, cx: &App) -> Div {
        let mut rows: Vec<Div> = FONT_SIZES
            .iter()
            .zip(&self.fonts)
            .enumerate()
            .map(|(index, (control, fonts))| {
                let select = anchored::select(fonts)
                    .accessibility_label(format!("{} font", control.label))
                    .search_placeholder("Find an installed font…");
                labelled(
                    control.label,
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(select))
                        .child(self.font_size(index)),
                )
            })
            .collect();
        // Tabs have a size of their own but use the headings font.
        rows.insert(1, self.tab_size_row(tokens));
        v_flex()
            .gap_3()
            .child(heading("Fonts", cx))
            .children(rows)
            .child(note(tokens, "Automatic uses the selected theme's default for each role. No fonts are downloaded."))
    }

    fn tab_size_row(&self, tokens: &Tokens) -> Div {
        labelled(
            FONT_SIZES[TAB_FONT_SIZE].label,
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(tokens.muted)
                        .child("Uses the headings font"),
                )
                .child(self.font_size(TAB_FONT_SIZE)),
        )
    }

    fn notifications(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let choices = [
            (
                "notification-completion",
                "Turn completion",
                self.notifications.completion,
                NotificationChoice::Completion,
            ),
            (
                "notification-waiting",
                "Waiting for input",
                self.notifications.waiting,
                NotificationChoice::Waiting,
            ),
            (
                "notification-os",
                "OS notifications",
                self.notifications.os,
                NotificationChoice::Os,
            ),
            (
                "notification-sound",
                "Sound",
                self.notifications.sound,
                NotificationChoice::Sound,
            ),
        ];
        let body = v_flex()
            .gap_3()
            .child(sources::sounds(self, tokens, cx))
            .child("Copilot")
            .child(note(
                tokens,
                "These event and delivery choices apply only to Copilot chat.",
            ))
            .children(choices.map(|(id, label, checked, choice)| {
                toggle_row(
                    label,
                    subspace_dashboard::toggle::switch(id, cx)
                        .checked(checked)
                        .on_click(cx.listener(move |settings, enabled: &bool, _, cx| {
                            settings.set_notification(choice, *enabled, cx);
                        })),
                )
            }));
        page_frame(
            "Choose how Copilot and Spaces notify you.",
            body.child(note(tokens, "OS delivery depends on system permission and packaging. Sound is independent of OS notifications."))
                .child(sources::render(self, tokens, cx)),
            tokens,
            cx,
        )
        .into_any_element()
    }
    fn page(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match &self.page {
            SettingsPage::General => general::render(self, tokens, cx),
            SettingsPage::Account => account::page(tokens, cx),
            SettingsPage::Chat => self.chat(tokens, cx),
            SettingsPage::Mcp => mcp::render(self, tokens, cx),
            SettingsPage::Appearance => self.appearance(tokens, cx),
            SettingsPage::Layout => layout::render(self, tokens, cx),
            SettingsPage::Terminal => terminal::render(self, tokens, cx),
            SettingsPage::Notifications => self.notifications(tokens, cx),
            #[cfg(debug_assertions)]
            SettingsPage::Developer => developer::render(self, tokens, cx),
            SettingsPage::Extension(id) => github_credentials::extension_page(self, id, tokens, cx),
            _ => pages::render(self, tokens, cx),
        }
    }
    /// The agent select and its note, shown while ACP is the selected engine.
    fn acp_agent_field(&self, tokens: &Tokens, cx: &App) -> Option<Div> {
        if self.selected_chat_engine(cx) != subspace_core::config::ChatEngine::Acp {
            return None;
        }
        Some(
            v_flex()
                .gap_3()
                .child(labelled(
                    text::ACP_AGENT,
                    anchored::select(&self.acp_agent).accessibility_label(text::ACP_AGENT),
                ))
                .child(note(tokens, help::ACP_AGENT)),
        )
    }

    fn chat(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let body = v_flex()
            .gap_3()
            .child(labelled(
                text::CHAT_ENGINE,
                anchored::select(&self.chat_engine).accessibility_label(text::CHAT_ENGINE),
            ))
            .child(note(tokens, help::CHAT_ENGINE))
            .children(self.acp_agent_field(tokens, cx))
            .child(
                toggle_row("Approve all tool requests for new chats", subspace_dashboard::toggle::switch("chat-approve-all-default", cx)
                    .checked(self.approve_all_default)
                    .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                        settings.approve_all_default = *enabled;
                        cx.notify();
                    }))),
            )
            .child(note(
                tokens,
                "Allows new Copilot chats to approve tool requests automatically, including full host and service access. This can be high risk: managed policy and service limits still apply, but review each request before enabling it. Pi and ACP agents do not support this mode; the saved choice is kept for Copilot.",
            ));
        page_frame(
            "Choose the engine and permission default for new chats.",
            body,
            tokens,
            cx,
        )
        .into_any_element()
    }
}

fn theme_directory_note(cx: &App) -> String {
    crate::product::gui_config_file(cx)
        .parent()
        .map(|dir| dir.join("themes"))
        .map_or_else(String::new, |path| {
            format!("{} {}", help::THEME_FILES, path.display())
        })
}

pub(super) fn note(tokens: &Tokens, message: impl Into<gpui_kit::SharedString>) -> Div {
    subspace_dashboard::fields::note(tokens, message)
}

pub(super) fn heading(text: impl Into<gpui_kit::SharedString>, cx: &App) -> Div {
    subspace_dashboard::fields::heading(text, cx)
}

/// A page's description and body. The dialog title's breadcrumb names the
/// page, so the page itself has no heading.
pub(super) fn page_frame(
    description: impl Into<gpui_kit::SharedString>,
    body: impl IntoElement,
    tokens: &Tokens,
    _cx: &App,
) -> Div {
    let description = description.into();
    v_flex()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .text_sm()
                .text_color(tokens.muted)
                .child(rich::rich_text(
                    "settings-page-description",
                    rich::Format::Markdown,
                    &description,
                )),
        )
        .child(body)
}

pub(super) fn labelled(label: impl Into<gpui_kit::SharedString>, control: impl IntoElement) -> Div {
    subspace_dashboard::fields::form_field(label, control)
}

#[cfg(test)]
mod tests {
    use super::body_height;

    #[test]
    fn the_body_height_stays_within_its_bounds() {
        assert_eq!(body_height(1000.), 740.);
        assert_eq!(body_height(800.), 540.);
        assert_eq!(body_height(600.), 340.);
        assert_eq!(body_height(100.), 40.);
    }
}
