//! The workspace: the navigator, the pane with its tab strip, toolbar and
//! content, and the inspector. The content is the shared list, or the open
//! decision's page, or why nothing could be read.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, KeyContext,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens, list, workspace};
use subspace_framework::lucide::IconName;

use crate::load::Failure;
use crate::state::Turn;
use crate::{Decisions, inspector, keys, navigator, page, toolbar};

impl Render for Decisions {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Nothing read for an account that is no longer signed in is drawn,
        // nor the search or menu that account had open.
        self.follow_account(cx);
        self.clear_controls(window, cx);
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
        .children(crate::author::view::render(self, cx))
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
        return page::view::render(this, open, &tokens, cx);
    }
    if let Some(failure) = this.listing.blocked() {
        return blocked(failure, &tokens, cx);
    }
    v_flex()
        .size_full()
        .child(
            div()
                .flex_1()
                .min_h_0()
                .child(list::render(this, window, cx)),
        )
        .children(
            (this.listing.more() || this.listing.pages.index() > 1)
                .then(|| paging(this, &tokens, cx)),
        )
        .into_any_element()
}

fn paging(this: &Decisions, tokens: &Tokens, cx: &mut Context<Decisions>) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .justify_end()
        .gap_2()
        .border_t_1()
        .border_color(tokens.line)
        .px_4()
        .py_2()
        .child(page_button(
            this,
            Turn::Previous,
            "decisions-previous",
            "Previous",
            cx,
        ))
        .child(
            div()
                .text_color(tokens.muted)
                .child(format!("Page {}", this.listing.pages.index())),
        )
        .child(page_button(this, Turn::Next, "decisions-next", "Next", cx))
}

fn page_button(
    this: &Decisions,
    turn: Turn,
    id: &'static str,
    label: &'static str,
    cx: &Context<Decisions>,
) -> impl IntoElement {
    action_button(id, Intent::Neutral, None, label)
        .aria_label(format!("{label} decisions page"))
        .disabled(!this.can_turn_page(turn))
        .on_click(cx.listener(move |this, _, _, cx| this.turn_page(turn, cx)))
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
