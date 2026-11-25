//! The open pull request as a page in the pane: the view's tab and the pull
//! request's closable tab, Back and the path in the toolbar with the review
//! actions, and one reading column with the title, the state, the rendered
//! description and the activity. The inspector stays beside it; while it is
//! hidden, its Properties, Reviewers and Checks end the reading column.
//!
//! The page is the shared list's open row: Enter, a double click, the
//! inspector's Open or a link from another Space opens it, and Escape, Back,
//! the tab's close or the view's tab closes it.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_framework::client_github::model::GhItem;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::list::{self, ListDashboard as _, ListState};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabBar, TabSpec};
use subspace_framework::dashboard::workspace::{Pane, READING_WIDTH_PX, Toolbar, text_edge};
use subspace_framework::dashboard::{
    CloseDetail, SelectNext, SelectPrevious, Tokens, heading_font,
};

use crate::inspector::{short_title, state_line};
use crate::peek::{self, Allowed};
use crate::{Github, detail, github_section_icon};

/// The page title's size before the theme scales it.
const PAGE_TITLE_PX: f32 = 22.;

/// Opens the page of pull request `key`, as Enter does on its row.
pub(crate) fn open_page(board: &mut Github, key: &str, cx: &mut Context<Github>) {
    list::update(board, cx, |list| {
        list.reveal(key);
    });
}

/// Back to the list, as Escape does.
pub(crate) fn close_page(board: &mut Github, cx: &mut Context<Github>) {
    list::update(board, cx, ListState::close_detail);
}

/// `j` and `k`: the next or previous pull request, keeping the page open
/// when one is.
fn step_selection(board: &mut Github, step: isize, cx: &mut Context<Github>) {
    list::update(board, cx, |list| {
        if list.open_row().is_some() {
            list.step_row(step);
        } else {
            list.step(step);
        }
    });
}

// qual:allow(coupling, oi) reason: "One file per concern of the PRs board, as its layout, inspector and repository filter files already are; this one owns the open pull request's page."
impl Github {
    /// The pane while pull request `key` is open.
    pub(crate) fn page_pane(&self, key: &SharedString, cx: &mut Context<Self>) -> Pane {
        let item = self.find_item(key).cloned();
        let tabs = self.page_tabs(key, item.as_ref(), cx);
        let toolbar = self.page_toolbar(item.as_ref(), cx);
        Pane::new("github-page-pane")
            .tabs(tabs)
            .toolbar(toolbar)
            .child(self.page_content(key, item.as_ref(), cx))
    }

    /// The view's tab, which goes back to the list, and the open pull
    /// request's, selected, whose close goes back too.
    fn page_tabs(&self, key: &str, item: Option<&GhItem>, cx: &Context<Self>) -> TabBar {
        let view = TabSpec::new(self.view_title(), github_section_icon(self.section));
        let view = dashboard_tabs::tab(view).on_click(cx.listener(|this, _, _, cx| {
            close_page(this, cx);
        }));
        // The repository and number as the key, then the title.
        let spec = match item {
            Some(item) => TabSpec::new(tab_title(&item.title), IconName::GitPullRequest)
                .key(short_title(item)),
            None => TabSpec::new(key.to_owned(), IconName::GitPullRequest),
        };
        let page = dashboard_tabs::tab(spec).on_close(cx.listener(|this, _, _, cx| {
            close_page(this, cx);
        }));
        dashboard_tabs::pane_tab_bar("gh-open-tabs")
            .selected_index(1)
            .child(view)
            .child(page)
    }

    /// Back and the path on the leading side, the review actions on the
    /// trailing side. The dock switches are the status bar's.
    fn page_toolbar(&self, item: Option<&GhItem>, cx: &mut Context<Self>) -> Toolbar {
        let name = format!("Back to {}", self.view_title());
        let back = icon_button("gh-page-back", Intent::Quiet, IconName::ArrowLeft, name)
            .on_click(cx.listener(|this, _, _, cx| close_page(this, cx)));
        let toolbar = Toolbar::new("github-page-toolbar").leading(back);
        match item {
            Some(item) => self.item_tools(toolbar, item, cx),
            None => toolbar,
        }
    }

    /// The open pull request's path, then the review actions its account
    /// may take.
    fn item_tools(&self, toolbar: Toolbar, item: &GhItem, cx: &mut Context<Self>) -> Toolbar {
        let detail = self
            .account_data()
            .and_then(|account| account.details.get(&item.key));
        let on = self.panel_actions();
        let allowed = Allowed::of(item, detail);
        let toolbar = peek::review_actions(allowed).into_iter().fold(
            toolbar.leading(path(item, cx)),
            |toolbar, action| {
                toolbar.trailing(self.item_action_button(action, "page", allowed.approved, &on))
            },
        );
        match crate::merge_pull_request::button(self, item, allowed.merge_is_primary(), cx) {
            Some(button) => toolbar.trailing(button),
            None => toolbar,
        }
    }

    /// The reading column, centred in the pane, scrolling.
    fn page_content(&self, key: &str, item: Option<&GhItem>, cx: &App) -> AnyElement {
        let tokens = Tokens::current(cx);
        let column = v_flex()
            .w_full()
            .min_w_0()
            .max_w(px(READING_WIDTH_PX))
            .px(text_edge(&tokens))
            .py_4()
            .gap_3();
        let column = match item {
            Some(item) => self.page_column(column, item, cx),
            None => self.pending_column(column, key, cx),
        };
        v_flex()
            .id("github-page")
            .test_support()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                v_flex()
                    .id("github-page-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(h_flex().w_full().justify_center().child(column)),
            )
            .into_any_element()
    }

    /// The state, the title, the description and the activity, then the
    /// inspector's sections while the inspector is hidden.
    fn page_column(&self, column: Div, item: &GhItem, cx: &App) -> Div {
        let tokens = Tokens::current(cx);
        let input = self.panel_input(item, &tokens);
        let column = column
            .child(state_line("gh-page-state", item, cx))
            .child(page_title("gh-page-title", item.title.clone(), cx))
            .child(detail::page_sections(&input, &tokens));
        match self.inspector_open {
            true => column,
            false => column.children(self.inspector_sections(item, cx)),
        }
    }

    /// A pull request opened from another Space that has not loaded: its
    /// reference and why it is not shown yet.
    fn pending_column(&self, column: Div, key: &str, cx: &App) -> Div {
        let tokens = Tokens::current(cx);
        let Some(detail) = self.detail(key) else {
            return column.child(page_title("gh-page-title", key.to_owned(), cx));
        };
        let sections = detail.sections.iter().enumerate().map(|(index, section)| {
            let id = SharedString::from(format!("gh-page-pending-{index}"));
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_color(tokens.muted)
                        .child(section.heading.clone()),
                )
                .child(list::render_body(id, &section.body, &tokens))
        });
        column
            .child(page_title("gh-page-title", detail.title.to_string(), cx))
            .children(sections)
    }
}

/// The page's title, in the heading font.
fn page_title(id: &'static str, title: String, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    div()
        .id(id)
        .aria_label(title.clone())
        .test_support()
        .font_family(heading_font(cx))
        .text_size(tokens.heading_size(PAGE_TITLE_PX))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(title)
        .into_any_element()
}

/// The path in the toolbar: the repository, then `#number`.
fn path(item: &GhItem, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let number = format!("#{}", item.number);
    h_flex()
        .id("gh-page-path")
        .aria_label(format!("{} {number}", item.repo))
        .test_support()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(item.repo.clone())
        .child("›")
        .child(div().text_color(tokens.fg_strong).child(number))
        .into_any_element()
}

/// Keys at the Space's root, for the page and the inspector, which sit
/// outside the shared list's own frame: Escape closes the page, and `j` and
/// `k` move to the next or previous pull request. Inside the list, the list
/// answers them first.
pub(crate) fn with_page_keys(root: Stateful<Div>, cx: &mut Context<Github>) -> Stateful<Div> {
    root.on_action(cx.listener(|this, _: &CloseDetail, _, cx| close_page(this, cx)))
        .on_action(cx.listener(|this, _: &SelectNext, _, cx| step_selection(this, 1, cx)))
        .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| step_selection(this, -1, cx)))
}

/// The most of a pull request's title its tab shows before it is cut.
const TAB_TITLE_CHARS: usize = 40;

/// A pull request's title on its tab, after its key, cut short so one long
/// title cannot crowd the tab strip.
fn tab_title(title: &str) -> String {
    let title = title.trim();
    match title.char_indices().nth(TAB_TITLE_CHARS) {
        Some((at, _)) => format!("{}\u{2026}", title[..at].trim_end()),
        None => title.to_owned(),
    }
}

#[cfg(test)]
mod tab_title_tests {
    use super::tab_title;

    #[test]
    fn a_long_title_is_cut_on_its_tab() {
        assert_eq!(tab_title(" Fix the relay "), "Fix the relay");
        let long = "Read the relay position after the correction is applied, every time";
        assert_eq!(
            tab_title(long),
            "Read the relay position after the correc\u{2026}"
        );
    }
}
