//! The workspace: the navigator, the pane with its tab strip, toolbar and
//! content, and the inspector. The content is the shared list, or the open
//! decision's page, or why nothing could be read.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, KeyContext,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div,
};
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens, list, workspace};
use subspace_framework::lucide::IconName;

use crate::load::Failure;
use crate::{Decisions, inspector, keys, navigator, page, toolbar};

impl Render for Decisions {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        // The shared list asks for its inset without the tokens, so it
        // reads the text edge this frame worked out.
        self.text_edge = workspace::text_edge(&tokens);
        let shell = shell(self, window, cx);
        // An element declares context names; `view_keys` is a binding
        // predicate and must not be parsed as one.
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DASHBOARD_CONTEXT);
        context.add(Self::ID);
        keys::handlers(
            v_flex()
                .size_full()
                .relative()
                .key_context(context)
                .track_focus(&self.focus),
            cx,
        )
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(div().flex_1().min_h_0().child(shell))
        .children(self.menu_anchors.view())
    }
}

/// The navigator and the inspector, unless hidden, beside the pane.
fn shell(
    this: &Decisions,
    window: &mut Window,
    cx: &mut Context<Decisions>,
) -> workspace::Workspace {
    let pane = workspace::Pane::new("decisions-pane")
        .tabs(toolbar::open_tabs(this, cx))
        .toolbar(toolbar::render(this, cx))
        .child(div().flex_1().min_h_0().child(content(this, window, cx)));
    let mut shell = workspace::Workspace::new("decisions-workspace").pane(pane);
    if this.docks.navigator {
        shell = shell.navigator(navigator::render(this, cx));
    }
    if this.docks.inspector {
        shell = shell.inspector(inspector::render(this, cx));
    }
    shell
}

fn content(this: &Decisions, window: &mut Window, cx: &mut Context<Decisions>) -> AnyElement {
    let tokens = Tokens::current(cx);
    if let Some(open) = this.shown_page() {
        return page::view::render(open, &tokens, cx);
    }
    if let Some(failure) = this.listing.blocked() {
        return blocked(failure, &tokens, cx);
    }
    list::render(this, window, cx).into_any_element()
}

/// Nothing could be read: why, and Retry. Signing in is under Settings ›
/// Account, which the words name.
fn blocked(failure: Failure, tokens: &Tokens, cx: &mut Context<Decisions>) -> AnyElement {
    let words = Words {
        chat: MenuChat::Off,
        ..Words::new(failure.title(), failure.message())
    };
    let retry = feedback::recovery_action(
        "decisions-retry",
        Intent::Neutral,
        IconName::RotateCw,
        "Retry",
        cx.listener(|this, _: &ClickEvent, _, cx| this.read(true, cx)),
    );
    div()
        .id("decisions-state")
        .test_support()
        .aria_label(failure.title())
        .size_full()
        .child(feedback::error_box_words(
            tokens,
            "decisions-state",
            words,
            [retry],
        ))
        .into_any_element()
}
