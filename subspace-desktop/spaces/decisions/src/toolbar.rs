//! The pane's tab strip and toolbar. The tab strip names what is open: the
//! current view and, beside it, an opened decision. The toolbar leads with
//! the view menu while the navigator is hidden, then search and the Tag
//! filter. The count and actions trail. The dock switches are
//! the status bar's (`Dashboard::docks`).

use gpui_kit::component::Icon;
use gpui_kit::{
    ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    div, px,
};
use subspace_api_client::decision_writes::DecisionReview;
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button, main_action};
use subspace_framework::dashboard::fields;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::tabs::{self, TabBar, TabSpec};
use subspace_framework::dashboard::toolbar_label::{self, ToolbarLabel};
use subspace_framework::dashboard::workspace;
use subspace_framework::lucide::IconName;

use crate::{Decisions, keys, menus, state};

/// The search field's width, as the Issues Space's toolbar has it.
const SEARCH_WIDTH_PX: f32 = 260.;

/// The list's tab, then the open decision's, which closes.
pub(crate) fn open_tabs(this: &Decisions, cx: &mut Context<Decisions>) -> TabBar {
    let view = this.choice.view;
    let bar = tabs::pane_tab_bar("decisions-open-tabs")
        .selected_index(usize::from(this.shown_page().is_some()))
        .child(tabs::tab(TabSpec::new(view.label(), view.icon())))
        .on_click(cx.listener(|this, index: &usize, _, cx| this.show_tab(*index, cx)));
    match &this.page {
        Some(page) => bar.child(
            tabs::tab(TabSpec::new(page.title.clone(), IconName::Scale))
                .aria_label(format!("Decision: {}", page.title))
                .on_close(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.close_page(window, cx);
                })),
        ),
        None => bar,
    }
}

pub(crate) fn render(this: &Decisions, cx: &mut Context<Decisions>) -> workspace::Toolbar {
    let mut toolbar = workspace::Toolbar::new("decisions-toolbar");
    // The navigator holds the views; hidden, they stay one menu away.
    if !this.docks.navigator {
        toolbar = toolbar.leading(menus::view_button(this, cx));
    }
    toolbar = toolbar.leading(search_field(this, cx));
    if let Some(tag) = menus::tag_button(this, cx) {
        toolbar = toolbar.leading(tag);
    }
    if let Some(count) = state::count_line(&this.listing) {
        toolbar = toolbar.trailing(toolbar_label::label(
            "decisions-count",
            ToolbarLabel {
                icon: Some(Icon::from(IconName::Scale)),
                accessible: count.clone().into(),
                text: count.into(),
            },
            cx,
        ));
    }
    trailing_actions(toolbar, this, cx)
}

fn trailing_actions(
    mut toolbar: workspace::Toolbar,
    this: &Decisions,
    cx: &mut Context<Decisions>,
) -> workspace::Toolbar {
    let can_review = this.shown_page().is_none() && this.review_item().is_some();
    if can_review && this.can_author() {
        toolbar = toolbar.trailing(new_decision(false, cx));
    }
    toolbar = toolbar.trailing(
        icon_button(
            "decisions-refresh",
            Intent::Quiet,
            IconName::RefreshCw,
            "Refresh",
        )
        .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
    );
    if can_review {
        return toolbar.trailing(
            main_action("decisions-accept", IconName::Check, "Accept").on_click(cx.listener(
                |this, _, window, cx| {
                    this.review_decision(DecisionReview::Accept, window, cx);
                },
            )),
        );
    }
    if this.can_author() {
        toolbar = toolbar.trailing(new_decision(true, cx));
    }
    toolbar
}

fn new_decision(primary: bool, cx: &Context<Decisions>) -> IconButton {
    let button = if primary {
        main_action("decisions-new", IconName::Plus, "New decision")
    } else {
        action_button(
            "decisions-new",
            Intent::Neutral,
            Some(IconName::Plus),
            "New decision",
        )
    };
    button.on_click(cx.listener(|this, _, window, cx| this.new_decision(false, window, cx)))
}

/// Typing searches the titles and text after a short pause; Enter or Escape
/// returns to the list.
fn search_field(this: &Decisions, cx: &Context<Decisions>) -> impl IntoElement {
    let search = fields::toolbar_search(&this.search, "Search decisions", cx);
    div()
        .key_context(keys::SEARCH_CONTEXT)
        .w(px(SEARCH_WIDTH_PX))
        .child(search.id("decisions-search"))
}
