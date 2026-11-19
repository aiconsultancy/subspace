//! Sign in with GitHub: the action and each state of a running sign-in,
//! with fixture codes only. Nothing here starts a sign-in.

use gpui_kit::{Context, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::github_sign_in::{self, Phase};

use super::{Sample, ignore_click, sample};
use crate::ComponentGallery;

pub(super) fn samples(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let phases = [
        ("Starting", "starting", Phase::Starting),
        ("Continue in the browser", "browser", Phase::Browser),
        (
            "Device code, with Copy code, Open GitHub and Cancel",
            "device",
            Phase::Device {
                user_code: "OCTO-1234".into(),
                verification_uri: "https://github.com/login/device".into(),
            },
        ),
        (
            "Failed",
            "failed",
            Phase::Failed("GitHub sign-in was declined in the browser.".into()),
        ),
    ];
    let mut samples = vec![
        sample(
            "Sign in with GitHub, ready",
            github_sign_in::sign_in_button("gallery-sample:github-sign-in", false)
                .on_click(ignore_click),
        ),
        sample(
            "Sign in with GitHub, while a sign-in runs",
            github_sign_in::sign_in_button("gallery-sample:github-sign-in-busy", true),
        ),
    ];
    samples.extend(phases.into_iter().filter_map(|(caption, key, phase)| {
        let id = format!("gallery-sample:github-sign-in-{key}");
        github_sign_in::status(&id, &phase, tokens, ignore_click, cx)
            .map(|status| sample(caption, status))
    }));
    samples
}
