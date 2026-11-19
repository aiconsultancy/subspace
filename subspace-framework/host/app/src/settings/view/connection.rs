//! Compose registered fields from the one shell-owned service draft.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, action_button, icon_button};
use subspace_dashboard::panel::IconName;
use subspace_dashboard::settings::ConnectionField;

use super::{Settings, github_credentials, labelled, note, text};

pub(super) fn render(
    settings: &Settings,
    fields: &[ConnectionField],
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    if let [ConnectionField::Note(message)] = fields {
        return note(tokens, *message).into_any_element();
    }
    v_flex()
        .gap_3()
        .children(
            fields
                .iter()
                .map(|spec| field_row(settings, *spec, locked, tokens, cx)),
        )
        .into_any_element()
}

fn field_row(
    settings: &Settings,
    spec: ConnectionField,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    match spec {
        ConnectionField::Token(token) => settings.token_field(token as usize, locked, tokens, cx),
        ConnectionField::GitHubCredentials => {
            github_credentials::render(settings, locked, tokens, cx).into_any_element()
        }
        ConnectionField::Repositories => {
            repository_fields(settings, locked, tokens, cx).into_any_element()
        }
        ConnectionField::Note(message) => note(tokens, message).into_any_element(),
    }
}

/// The favourite repositories: one row each with Remove, then Add
/// repository…, which opens the shared search. Edits apply on Save.
fn repository_fields(
    settings: &Settings,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> impl gpui_kit::IntoElement {
    let list = settings.form.repos.clone();
    let favourites = list.read(cx).favourites().to_vec();
    let rows: Vec<AnyElement> = favourites
        .iter()
        .map(|repo| repository_row(repo, locked, tokens, cx))
        .collect();
    let empty = rows
        .is_empty()
        .then(|| note(tokens, "No favourite repositories yet."));
    let add = action_button(
        "settings-repositories-add-button",
        Intent::Neutral,
        Some(IconName::Plus),
        "Add repository…",
    )
    .disabled(locked)
    .on_click(cx.listener(|settings, _, window, cx| settings.add_repository(window, cx)));
    let trigger = list.update(cx, |list, cx| {
        list.anchors()
            .trigger(super::super::repositories::ADD_ANCHOR, add, cx)
    });
    let host = list.read(cx).anchors().view();
    labelled(
        text::REPOS_LABEL,
        v_flex()
            .id("settings-repositories-list")
            .test_support()
            .role(Role::Group)
            .aria_label("Repositories")
            .w_full()
            .gap_1()
            .children(rows)
            .children(empty)
            .child(h_flex().child(trigger))
            .children(host),
    )
    .id("settings-repositories")
    .test_support()
    .child(note(
        tokens,
        "Shared by PRs (All open), Copilot Agents, Actions and the agent tools. The PRs Space stars them too. Save applies changes.",
    ))
}

fn repository_row(
    repo: &str,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let name = repo.to_owned();
    h_flex()
        .id(ElementId::Name(
            format!("settings-repository:{repo}").into(),
        ))
        .gap_2()
        .min_h_6()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(tokens.fg)
                .child(repo.to_owned()),
        )
        .child(
            icon_button(
                ElementId::Name(format!("settings-repository-remove:{repo}").into()),
                Intent::Quiet,
                IconName::X,
                format!("Remove {repo}"),
            )
            .disabled(locked)
            .on_click(cx.listener(move |settings, _, _, cx| settings.remove_repository(&name, cx))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use subspace_dashboard::settings::ConnectionToken;

    #[test]
    fn registered_tokens_use_the_original_shared_editor_identity() {
        assert_eq!(ConnectionToken::GitHub as usize, 0);
    }
}
