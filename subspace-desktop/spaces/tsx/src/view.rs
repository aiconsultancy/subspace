//! The workspace: the navigator lists the installed packages, the pane's
//! toolbar holds Install, Reload and Remove, and the content is the
//! selected package's own view or why it is not running.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _,
    Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::action::{self, Intent};
use subspace_framework::dashboard::feedback;
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens, workspace};
use subspace_framework::lucide::IconName;
use subspace_tsx_system::Tier;

use crate::fields::plain_words;
use crate::store::Installed;
use crate::{Content, TsxSpaces};

pub(crate) fn render(
    this: &mut TsxSpaces,
    window: &mut Window,
    cx: &mut Context<TsxSpaces>,
) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
    context.add(DASHBOARD_CONTEXT);
    context.add(TsxSpaces::ID);
    let pane = workspace::Pane::new("tsx-pane")
        .toolbar(toolbar(this, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .size_full()
                .child(content(this, &tokens, window, cx)),
        );
    let mut shell = workspace::Workspace::new("tsx-workspace").pane(pane);
    if this.navigator {
        shell = shell.navigator(navigator(this, cx));
    }
    v_flex()
        .size_full()
        .key_context(context)
        .track_focus(&this.focus)
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(div().flex_1().min_h_0().child(shell))
}

fn navigator(this: &TsxSpaces, cx: &mut Context<TsxSpaces>) -> workspace::Navigator {
    let rows = this.packages.iter().flatten().map(|package| {
        let file = package.file.clone();
        workspace::NavRow::new(file.display().to_string(), package_name(package))
            .icon(IconName::Package)
            .count(status_word(this, package))
            .selected(this.selected.as_ref() == Some(&package.file))
            .on_click(cx.listener(move |this, _, window, cx| this.select(file.clone(), window, cx)))
    });
    workspace::Navigator::new(TsxSpaces::TITLE)
        .badge("Experimental")
        .child(workspace::nav_section("Packages", cx))
        .children(rows)
}

fn package_name(package: &Installed) -> String {
    match &package.state {
        Ok(verified) => verified.name.clone(),
        Err(_) => package
            .file
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned()),
    }
}

/// The row's status: for the selected package, the state the content pane
/// shows; for the others, whether they can start.
fn status_word(this: &TsxSpaces, package: &Installed) -> &'static str {
    let Ok(verified) = &package.state else {
        return "Error";
    };
    if this.selected.as_ref() == Some(&package.file) {
        return content_word(&this.content());
    }
    if !verified.identity.declared.is_empty() && verified.identity.granted(&this.saved).is_empty() {
        "Needs grant"
    } else {
        "Ready"
    }
}

fn content_word(content: &Content) -> &'static str {
    match content {
        Content::Running => "Running",
        Content::NeedsGrant(_) => "Needs grant",
        Content::LaunchFailed(_) => "Could not start",
        Content::Failed(_) => "Stopped",
        Content::Refused(_) => "Error",
        Content::Loading | Content::Empty => "Ready",
    }
}

fn toolbar(this: &TsxSpaces, cx: &mut Context<TsxSpaces>) -> workspace::Toolbar {
    let install = action::toolbar_button(
        "tsx-install",
        IconName::PackagePlus,
        "Install package",
        (Intent::Primary, true),
        cx,
    )
    .on_click(cx.listener(|this, _, window, cx| this.install(window, cx)));
    let reload = action::toolbar_button(
        "tsx-reload",
        IconName::RotateCw,
        "Reload",
        (Intent::Neutral, false),
        cx,
    )
    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx)));
    let mut bar = workspace::Toolbar::new("tsx-toolbar")
        .trailing(install)
        .trailing(reload);
    if this.selected.is_some() {
        let remove = action::toolbar_button(
            "tsx-remove",
            IconName::Trash,
            "Remove package",
            (Intent::Neutral, false),
            cx,
        )
        .on_click(cx.listener(|this, _, window, cx| this.remove(window, cx)));
        bar = bar.trailing(remove);
    }
    bar
}

fn content(
    this: &TsxSpaces,
    tokens: &Tokens,
    _window: &mut Window,
    cx: &mut Context<TsxSpaces>,
) -> AnyElement {
    let body = match (this.content(), &this.running) {
        (Content::Running, Some(running)) => running.view.clone().into_any_element(),
        (content, _) => div()
            .id("tsx-state")
            .test_support()
            .size_full()
            .child(state(content, tokens, cx))
            .into_any_element(),
    };
    v_flex()
        .size_full()
        .children(
            this.notice
                .clone()
                .map(|notice| notice_banner(tokens, notice, cx)),
        )
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}

fn state(content: Content, tokens: &Tokens, cx: &mut Context<TsxSpaces>) -> gpui_kit::Div {
    match content {
        Content::Running => div(),
        Content::Loading => feedback::empty(
            tokens,
            "tsx-loading",
            "Checking packages",
            "Verifying each package's signature.",
        ),
        Content::Empty => empty(tokens, cx),
        Content::Refused(reason) => feedback::error_box_with_actions(
            tokens,
            "tsx-refused",
            "This package cannot run",
            reason,
            [feedback::configure_action("tsx-refused-configure")],
        ),
        Content::NeedsGrant(tiers) => feedback::empty_with_actions(
            tokens,
            "tsx-grant",
            "Review what this package asks for",
            grant_hint(&tiers),
            [feedback::configure_action("tsx-grant-configure")],
        ),
        Content::LaunchFailed(reason) => feedback::error_box(
            tokens,
            "tsx-launch",
            "The package could not start",
            reason,
            cx.listener(|this, _, window, cx| this.reload(window, cx)),
        ),
        Content::Failed(failure) => feedback::error_box(
            tokens,
            "tsx-failed",
            "The package stopped",
            failure,
            cx.listener(|this, _, window, cx| this.reload(window, cx)),
        ),
    }
}

fn empty(tokens: &Tokens, cx: &mut Context<TsxSpaces>) -> gpui_kit::Div {
    let install = feedback::recovery_action(
        "tsx-empty-install",
        Intent::Primary,
        IconName::PackagePlus,
        "Install package",
        cx.listener(|this, _, window, cx| this.install(window, cx)),
    );
    feedback::empty_with_actions(
        tokens,
        "tsx-empty",
        "No TSX Spaces installed",
        "Install a signed .nshpkg package. First add the root key that signed it under Configure.",
        [install, feedback::configure_action("tsx-empty-configure")],
    )
}

/// What the grant review says: each capability the package asks for.
pub(crate) fn grant_hint(tiers: &[Tier]) -> String {
    let asks: Vec<&str> = tiers.iter().map(|tier| plain_words(*tier)).collect();
    format!(
        "It asks to {}. Allow what you want under Configure and save; nothing is read before that.",
        asks.join("; ")
    )
}

/// Why the last install, removal or build record failed, until dismissed.
fn notice_banner(tokens: &Tokens, notice: String, cx: &mut Context<TsxSpaces>) -> gpui_kit::Div {
    let dismiss = feedback::recovery_action(
        "tsx-notice-dismiss",
        Intent::Neutral,
        IconName::X,
        "Dismiss",
        cx.listener(|this, _, _, cx| {
            this.notice = None;
            cx.notify();
        }),
    );
    feedback::error_banner_with_actions(tokens, "tsx-notice", "Not done", notice, [dismiss])
}

/// The status bar: the selected package, its version and signer, and what
/// it may read.
pub(crate) fn status_line(this: &TsxSpaces) -> Option<String> {
    let verified = this.selected_package()?.state.as_ref().ok()?;
    let granted = verified.identity.granted(&this.saved);
    let reads = if granted.is_empty() {
        "nothing granted".to_owned()
    } else {
        format!(
            "{} of {} capabilities granted",
            granted.len(),
            verified.identity.declared.len()
        )
    };
    Some(format!(
        "{} {} · signed by a trusted root · {reads}",
        verified.name, verified.version
    ))
}

#[cfg(test)]
mod tests {
    use super::grant_hint;
    use subspace_tsx_system::Tier;

    #[test]
    fn the_selected_row_says_what_the_content_pane_shows() {
        use super::{Content, content_word};
        assert_eq!(content_word(&Content::Failed("x".into())), "Stopped");
        assert_eq!(
            content_word(&Content::LaunchFailed("x".into())),
            "Could not start"
        );
        assert_eq!(content_word(&Content::Running), "Running");
    }

    #[test]
    fn the_grant_review_names_each_capability_in_plain_words() {
        let hint = grant_hint(&[Tier::Metrics, Tier::Apps]);
        assert!(hint.contains("read CPU, memory and network use"));
        assert!(hint.contains("read which apps use the most memory"));
        assert!(hint.contains("nothing is read before that"));
    }
}
