//! The Sign in with GitHub action and the component that shows a running
//! sign-in.

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_lucide::IconName;

use super::Phase;
use crate::action::{Intent, action_button};
use crate::icon_button::IconButton;
use crate::{Tokens, copy_details, feedback, links, rich};

/// The device code's size at the default monospace setting: larger than
/// body text, so it reads at a glance.
const CODE_SIZE: f32 = 18.;

/// The Sign in with GitHub action. The owner starts the shared sign-in.
pub fn sign_in_button(id: impl Into<ElementId>, busy: bool) -> IconButton {
    use gpui_kit::component::Disableable as _;
    action_button(
        id,
        Intent::Neutral,
        Some(IconName::LogIn),
        "Sign in with GitHub",
    )
    .disabled(busy)
}

/// What the running sign-in needs from the user, or why the last one
/// failed; nothing while idle. `id` prefixes every element ID.
pub fn status(
    id: &str,
    phase: &Phase,
    tokens: &Tokens,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let name = |part: &str| ElementId::Name(format!("{id}-{part}").into());
    let cancel = action_button(name("cancel"), Intent::Quiet, Some(IconName::X), "Cancel")
        .on_click(on_cancel);
    match phase {
        Phase::Idle => None,
        Phase::Starting => Some(waiting(
            name("message"),
            "Starting sign-in…",
            tokens,
            cancel,
        )),
        Phase::Browser => Some(waiting(
            name("message"),
            "Continue in your browser. Subspace finishes signing in when GitHub sends you back.",
            tokens,
            cancel,
        )),
        Phase::Device {
            user_code,
            verification_uri,
        } => Some(device(
            &name,
            (user_code, verification_uri),
            tokens,
            cancel,
            cx,
        )),
        Phase::Failed(message) => Some(
            div()
                .id(name("error"))
                .test_support()
                .text_color(tokens.red)
                .child(rich::plain_text(name("error-text"), message))
                .into_any_element(),
        ),
    }
}

/// A waiting line with Cancel under it.
fn waiting(id: ElementId, message: &str, tokens: &Tokens, cancel: IconButton) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(feedback::loading_line(tokens, id, message.to_owned()))
        .child(h_flex().child(cancel))
        .into_any_element()
}

/// The code to enter, with Copy code, Open GitHub and Cancel.
fn device(
    name: &dyn Fn(&str) -> ElementId,
    (code, uri): (&str, &str),
    tokens: &Tokens,
    cancel: IconButton,
    cx: &App,
) -> AnyElement {
    let copied = code.to_owned();
    let address = uri.to_owned();
    let mono = cx.theme().mono_font_family.clone();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(feedback::loading_line(
            tokens,
            name("message"),
            "Enter this code on GitHub. Subspace finishes signing in when GitHub confirms it.",
        ))
        .child(
            div()
                .id(name("code"))
                .test_support()
                .font_family(mono)
                .text_size(tokens.mono_size(CODE_SIZE))
                .text_color(tokens.fg_strong)
                .child(rich::plain_text(name("code-text"), code)),
        )
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(copy_details::copy_text(
                    name("copy"),
                    "Copy code",
                    "Code copied",
                    move |_| copied.clone(),
                ))
                .child(
                    action_button(
                        name("open"),
                        Intent::Neutral,
                        Some(IconName::ExternalLink),
                        "Open GitHub",
                    )
                    .on_click(move |_, _, cx| {
                        links::open(&address, cx);
                    }),
                )
                .child(cancel),
        )
        .into_any_element()
}
