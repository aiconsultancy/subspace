//! The inspector beside the list or the page: the selected pull request's
//! state, title and excerpt, its facts, reviewers and checks, and the
//! actions on it in the footer. One click on a row selects it and fills the
//! inspector; Open, a double click or Enter opens the page.

use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::client_github::model::{GhCheck, GhItem, GhItemDetail, ItemKind};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::list::{self, ListDashboard as _};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::workspace::{self, Inspector};
use subspace_framework::dashboard::{Tokens, anchored, avatar, feedback, heading_font, links};

use crate::Github;
use crate::detail::OnAction;
use crate::model::{CHECKS_SHOWN, check_summary};
use crate::peek::{self, Allowed, ItemAction};
use crate::rows::{state_glyph, state_label};

mod status_rows;

pub(crate) use status_rows::link_label;
use status_rows::{reviewers, status_row};

const INSPECTOR_ID: &str = "github-inspector";
/// The inspector's title before a pull request is selected.
const NO_SELECTION_TITLE: &str = "Pull request";
const NO_SELECTION: &str = "Select a pull request to see it here.";
const CLOSE_LABEL: &str = "Close inspector";

/// `api #421`: the repository's short name and the number.
pub(crate) fn short_title(item: &GhItem) -> String {
    let short = item.repo.rsplit('/').next().unwrap_or(&item.repo);
    format!("{short} #{}", item.number)
}

// qual:allow(coupling, oi) reason: "One file per concern of the PRs board, as its layout, page and repository filter files already are; this one owns the inspector."
impl Github {
    /// The inspector for the selected (or open) pull request.
    pub(crate) fn inspector(&self, cx: &mut Context<Self>) -> Inspector {
        let close = cx.listener(|this, _, _, cx| this.toggle_inspector(cx));
        let key = self.list.selected_row().map(|row| row.key.to_string());
        let Some(key) = key else {
            return empty_inspector(cx).on_close(CLOSE_LABEL, close);
        };
        let Some(item) = self.find_item(&key).cloned() else {
            return self
                .pending_inspector(&key, cx)
                .on_close(CLOSE_LABEL, close);
        };
        let mut inspector = Inspector::new(INSPECTOR_ID, short_title(&item));
        if self.list.open_row().is_none() {
            inspector = inspector.action(open_button(key, cx));
        }
        let footer = self.inspector_footer(&item, cx);
        inspector
            .action(github_button(item.url.clone()))
            .on_close(CLOSE_LABEL, close)
            .child(self.lead(&item, cx))
            .children(self.inspector_sections(&item, cx))
            .footer(footer)
    }

    /// A pull request opened from another Space that has not loaded: its
    /// reference, and why it is not shown yet.
    fn pending_inspector(&self, key: &str, cx: &App) -> Inspector {
        let tokens = Tokens::current(cx);
        let Some(detail) = self.detail(key) else {
            return empty_inspector(cx);
        };
        let sections = detail.sections.iter().enumerate().map(|(index, section)| {
            let id = SharedString::from(format!("gh-peek-pending-{index}"));
            workspace::section(section.heading.clone(), cx).child(list::render_body(
                id,
                &section.body,
                &tokens,
            ))
        });
        Inspector::new(INSPECTOR_ID, detail.id.clone()).children(sections)
    }

    /// The state, the title, then the description's first paragraph, the
    /// read error, or that the details are loading.
    fn lead(&self, item: &GhItem, cx: &App) -> Div {
        let tokens = Tokens::current(cx);
        let detail = self.item_detail_of(&item.key);
        let error = self
            .account_data()
            .and_then(|account| account.detail_errors.get(&item.key))
            .cloned();
        let note = match (detail, error) {
            (Some(detail), _) => peek::excerpt(&detail.body_html).map(|text| {
                div()
                    .id("gh-peek-excerpt")
                    .test_support()
                    .text_color(tokens.fg)
                    .child(text)
                    .into_any_element()
            }),
            (None, Some(error)) => {
                Some(div().text_color(tokens.red).child(error).into_any_element())
            }
            (None, None) => Some(
                feedback::loading_line(&tokens, "gh-peek-loading", "Loading…").into_any_element(),
            ),
        };
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(state_line("gh-peek-state", item, cx))
            .child(
                div()
                    .id("gh-peek-title")
                    .aria_label(item.title.clone())
                    .test_support()
                    .font_family(heading_font(cx))
                    .text_size(tokens.heading_font)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tokens.fg_strong)
                    .child(item.title.clone()),
            )
            .children(note)
    }

    /// Properties, Reviewers and Checks: in the inspector, or at the end of
    /// the page while the inspector is hidden.
    pub(crate) fn inspector_sections(&self, item: &GhItem, cx: &App) -> Vec<AnyElement> {
        let detail = self.item_detail_of(&item.key);
        let mut sections = vec![self.properties(item, detail, cx)];
        if item.kind == ItemKind::Pr {
            sections.push(reviewers(detail, cx));
        }
        if let Some(detail) = detail.filter(|detail| !detail.check_runs.is_empty()) {
            sections.push(self.checks(&item.key, &detail.check_runs, cx));
        }
        sections
    }

    /// The loaded details of `key`, if any.
    fn item_detail_of(&self, key: &str) -> Option<&GhItemDetail> {
        self.account_data()
            .and_then(|account| account.details.get(key))
    }

    fn properties(&self, item: &GhItem, detail: Option<&GhItemDetail>, cx: &App) -> AnyElement {
        let tokens = Tokens::current(cx);
        let mono = cx.theme().mono_font_family.clone();
        let facts = peek::facts(item, detail, self.now).into_iter().map(|fact| {
            let value = match (fact.label, fact.mono) {
                ("Author", _) => h_flex()
                    .gap_2()
                    .min_w_0()
                    .child(avatar::member(fact.value.clone(), &tokens))
                    .child(fact.value)
                    .into_any_element(),
                (_, true) => div()
                    .font_family(mono.clone())
                    .child(fact.value)
                    .into_any_element(),
                _ => div().child(fact.value).into_any_element(),
            };
            workspace::fact(fact.label, value, cx)
        });
        workspace::section("Properties", cx)
            .children(facts)
            .into_any_element()
    }

    /// Checks · n: one row per check, collapsed to the failing ones above
    /// `CHECKS_SHOWN` until all are asked for.
    fn checks(&self, key: &str, runs: &[GhCheck], cx: &App) -> AnyElement {
        let tokens = Tokens::current(cx);
        let show_all = self.checks_open.contains(key);
        let toggle = (runs.len() > CHECKS_SHOWN).then(|| self.checks_toggle(key, runs, show_all));
        let rows = peek::check_rows(runs, show_all)
            .into_iter()
            .enumerate()
            .map(|(index, row)| status_row("gh-check", index, row, false, &tokens));
        workspace::section(format!("Checks · {}", runs.len()), cx)
            .children(toggle)
            .children(rows)
            .into_any_element()
    }

    /// The line that shows all checks or only the failing ones.
    fn checks_toggle(&self, key: &str, runs: &[GhCheck], show_all: bool) -> AnyElement {
        let verb = if show_all { "show failing" } else { "show all" };
        let weak = self.weak.clone();
        let key = key.to_owned();
        action_button(
            "gh-checks-toggle",
            Intent::Quiet,
            Some(IconName::ListChecks),
            format!("{} · {verb}", check_summary(runs)),
        )
        .selected(show_all)
        .toggled(show_all)
        .on_click(move |_, _, cx| {
            _ = weak.update(cx, |this, cx| {
                if !this.checks_open.remove(&key) {
                    this.checks_open.insert(key.clone());
                }
                cx.notify();
            });
        })
        .into_any_element()
    }

    /// The footer: the composer while one is open for this pull request,
    /// else the actions the account may take, which wrap in a narrow dock.
    fn inspector_footer(&self, item: &GhItem, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::current(cx);
        if let Some(composer) = self.composer_element(&item.key, &tokens) {
            return composer;
        }
        let allowed = Allowed::of(item, self.item_detail_of(&item.key));
        let on = self.panel_actions();
        h_flex()
            .id("gh-peek-actions")
            .w_full()
            .min_w_0()
            .flex_wrap()
            .gap_2()
            .children(
                peek::footer_actions(allowed)
                    .into_iter()
                    .map(|action| self.item_action_button(action, "top", allowed.approved, &on)),
            )
            .children(
                (self.list.open_row().is_none())
                    .then(|| {
                        crate::merge_pull_request::button(
                            self,
                            item,
                            allowed.merge_is_primary(),
                            cx,
                        )
                    })
                    .flatten(),
            )
            .into_any_element()
    }

    /// One action's button. `place` keeps the ids of the inspector's buttons
    /// (`top`) apart from the page toolbar's (`page`).
    pub(crate) fn item_action_button(
        &self,
        action: ItemAction,
        place: &'static str,
        approved: bool,
        on: &OnAction,
    ) -> AnyElement {
        let look = peek::look(action, approved);
        let on = on.clone();
        let panel_action = look.action;
        let button = action_button(
            (look.id, place.len()),
            look.intent,
            Some(look.icon),
            look.label,
        )
        .accessibility_label(look.name)
        .disabled(self.writing)
        .on_click(move |_, window, cx| on(panel_action, window, cx));
        match action {
            ItemAction::Assign => {
                anchored::trigger(&self.assignee_anchor, button).into_any_element()
            }
            _ => button.into_any_element(),
        }
    }
}

/// The state's glyph and word, in the state's tone.
pub(crate) fn state_line(id: &'static str, item: &GhItem, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let glyph = state_glyph(item);
    let colour = tokens.glyph(&glyph);
    let label = state_label(item);
    h_flex()
        .id(id)
        .aria_label(label)
        .test_support()
        .gap_1p5()
        .text_size(tokens.secondary_size())
        .text_color(colour)
        .child(glyph.text.clone())
        .child(label)
        .into_any_element()
}

fn empty_inspector(cx: &App) -> Inspector {
    let tokens = Tokens::current(cx);
    Inspector::new(INSPECTOR_ID, NO_SELECTION_TITLE).child(
        div()
            .id("gh-peek-empty")
            .test_support()
            .child(feedback::empty_line(
                &tokens,
                "gh-peek-empty-text",
                NO_SELECTION,
            )),
    )
}

/// Open: the pull request's page.
fn open_button(key: String, cx: &Context<Github>) -> AnyElement {
    action_button(
        "gh-peek-open",
        Intent::Neutral,
        Some(IconName::ArrowUpRight),
        "Open",
    )
    .on_click(cx.listener(move |this, _, _, cx| crate::page::open_page(this, &key, cx)))
    .into_any_element()
}

/// The pull request's page on GitHub.
fn github_button(url: String) -> AnyElement {
    icon_button(
        "gh-peek-github",
        Intent::Quiet,
        IconName::Github,
        "Open on GitHub",
    )
    .on_click(move |_, _, cx| {
        links::open(&url, cx);
    })
    .into_any_element()
}
