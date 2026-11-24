#[cfg(test)]
mod header_tests;
mod sources;

use gpui_kit::component::list::ListItem;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    App, Context, Div, Global, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, actions, div,
};
use subspace_lucide::IconName;

use super::{SearchView, SearchViewEvent, policy};
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
    fields::{self, InputRole},
    icon_button::IconButton,
    rich,
    surface_header::SurfaceHeader,
    tabs::{self, TabSpec},
};

actions!(subspace_full_text, [Next, Previous, Activate, Close]);
struct Keys;
impl Global for Keys {}

pub(super) fn bind_keys(cx: &mut App) {
    if cx.has_global::<Keys>() {
        return;
    }
    cx.set_global(Keys);
    cx.bind_keys([
        KeyBinding::new("down", Next, Some("FullTextQuery > Input")),
        KeyBinding::new("up", Previous, Some("FullTextQuery > Input")),
        KeyBinding::new("enter", Activate, Some("FullTextQuery > Input")),
        KeyBinding::new("escape", Close, Some("FullTextSearch")),
        KeyBinding::new("escape", Close, Some("FullTextSearch > Input")),
    ]);
}

impl Render for SearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drop_hidden();
        let t = Tokens::current(cx);
        v_flex()
            .id("full-text-search")
            .key_context("FullTextSearch")
            .size_full()
            .min_h_0()
            .bg(t.bg)
            .text_color(t.fg)
            .text_size(t.row_font)
            .on_action(cx.listener(|this, _: &Close, _, cx| {
                if this.confirm_clear.take().is_some() {
                    cx.notify();
                } else {
                    cx.emit(SearchViewEvent::Close);
                }
            }))
            .child(self.header(cx))
            .children(self.error.clone().map(|error| {
                div()
                    .px(t.pad_x)
                    .py_2()
                    .text_color(t.red)
                    .child(rich::plain_text("search-error", &error))
            }))
            .children(self.notice.clone().map(|notice| {
                div()
                    .px(t.pad_x)
                    .py_2()
                    .text_color(t.muted)
                    .child(rich::plain_text("search-notice", &notice))
            }))
            .child(if self.managing {
                self.source_panel(cx)
            } else {
                self.results(cx)
            })
    }
}

impl SearchView {
    fn header(&self, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        v_flex()
            .flex_shrink_0()
            .child(
                SurfaceHeader::new(
                    IconName::Search,
                    "Search",
                    "Close search",
                    cx.listener(|_, _, _, cx| cx.emit(SearchViewEvent::Close)),
                )
                .description("Find words in content cached on this device.")
                .action(self.sources_button(cx)),
            )
            .child(
                v_flex()
                    .px(t.pad_x)
                    .py_3()
                    .gap_3()
                    .border_b_1()
                    .border_color(t.line)
                    .child(self.query_bar(cx))
                    .child(self.scope_bar(cx)),
            )
    }

    fn sources_button(&self, cx: &mut Context<Self>) -> IconButton {
        let label = if self.managing {
            "Back to results"
        } else {
            "Sources & model"
        };
        action_button(
            "search-sources",
            Intent::Neutral,
            Some(IconName::Database),
            label,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.managing = !this.managing;
            cx.notify();
        }))
    }

    fn query_bar(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .key_context("FullTextQuery")
            .on_action(cx.listener(|this, _: &Next, _, cx| this.move_selection(true, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.move_selection(false, cx)))
            .on_action(cx.listener(|this, _: &Activate, window, cx| {
                if let Some(index) = this.selected {
                    this.open_result(index, window, cx);
                } else {
                    this.submit(window, cx);
                }
            }))
            .child(div().flex_1().child(fields::input(
                &self.input,
                InputRole::Search,
                "Search cached content",
            )))
            .child(
                action_button(
                    "search-submit",
                    Intent::Primary,
                    Some(IconName::Search),
                    "Search",
                )
                .disabled(self.busy)
                .loading(self.loading)
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }

    fn scope_bar(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .justify_between()
            .gap_3()
            .child(
                tabs::tab_bar("search-scope")
                    .selected_index(usize::from(self.all))
                    .child(tabs::tab(
                        TabSpec::new(self.board_name(&self.current), IconName::LayoutDashboard)
                            .disabled(!policy::participates(&self.sources, &self.current)),
                    ))
                    .child(tabs::tab(TabSpec::new("All boards", IconName::Layers)))
                    .on_click(cx.listener(|this, index: &usize, window, cx| {
                        this.all = *index == 1;
                        this.submit(window, cx);
                    })),
            )
            .child(
                action_button(
                    "search-rerank",
                    Intent::Quiet,
                    Some(IconName::Sparkles),
                    "Local rerank",
                )
                .tooltip("Local rerank")
                .latched(self.rerank)
                .disabled(!self.model.as_ref().is_some_and(|m| m.installed))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.rerank = !this.rerank;
                    this.submit(window, cx);
                })),
            )
    }

    fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        self.selected = policy::step(
            self.selected,
            self.response.as_ref().map_or(0, |r| r.hits.len()),
            down,
        );
        if let Some(index) = self.selected {
            self.scroll.scroll_to_item(index);
        }
        cx.notify();
    }

    fn results(&self, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        let Some(response) = &self.response else {
            let (title, detail) = if self.loading {
                (
                    "Searching local content…",
                    "Your query stays on this device.",
                )
            } else if !self.sources.iter().any(|s| s.enabled) {
                (
                    "Choose what to keep",
                    "Open Sources & model to enable caching for a board.",
                )
            } else {
                (
                    "Search your cached work",
                    "Enter words, then press Enter. Only successfully fetched content is indexed.",
                )
            };
            return v_flex()
                .flex_1()
                .px(t.pad_x)
                .py_6()
                .gap_2()
                .child(
                    div()
                        .text_size(t.heading_font)
                        .child(rich::plain_text("search-empty-title", title)),
                )
                .child(
                    div()
                        .text_color(t.muted)
                        .child(rich::plain_text("search-empty-detail", detail)),
                );
        };
        v_flex().flex_1().min_h_0().gap_2().px(t.pad_x).py_4()
            .child(div().text_color(t.muted).child(format!("{}{} results · {} · {} enabled sources · ↑ ↓ select · Enter open",
                response.hits.len(), if response.truncated { "+" } else { "" }, response.ranking,
                response.sources.iter().filter(|s| s.enabled).count())))
            .children(response.warning.clone().map(|warning| div().text_color(t.muted).child(rich::plain_text("search-warning", &warning))))
            .child(div().text_color(t.muted).child(rich::plain_text("search-coverage", "Coverage is partial: fetched content only, within each source’s retention and storage limits.")))
            .children(response.hits.is_empty().then(|| div().py_4().child(rich::plain_text("search-no-results", "No matching cached content. Try fewer words, all boards, or check source coverage."))))
            .child(v_flex().id("search-results").flex_1().min_h_0().overflow_y_scroll().gap_2()
                .track_scroll(&self.scroll)
                .children(response.hits.iter().enumerate().map(|(index, hit)| self.result_row(index, hit, cx))))
    }

    fn result_row(
        &self,
        index: usize,
        hit: &crate::full_text::Hit,
        cx: &mut Context<Self>,
    ) -> ListItem {
        let t = Tokens::current(cx);
        let refreshed = chrono::DateTime::from_timestamp(hit.document.refreshed_at, 0).map_or_else(
            || "unknown age".into(),
            |date| subspace_dashboard::dates::relative_time(&date.to_rfc3339(), chrono::Utc::now()),
        );
        let source = format!(
            "{} · {} · {} · cached {}",
            self.board_name(&hit.target.scope.board),
            hit.target.scope.endpoint,
            hit.document.kind,
            refreshed
        );
        ListItem::new(("search-result", index))
            .selected(self.selected == Some(index))
            .accessibility_label(format!("{}. {}", hit.document.title, source))
            .py_3()
            .child(
                v_flex()
                    .gap_1()
                    .w_full()
                    .child(
                        div()
                            .text_color(t.fg_strong)
                            .child(hit.document.title.clone()),
                    )
                    .child(div().text_color(t.muted).child(source))
                    .child(div().child(hit.excerpt.clone())),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_result(index, window, cx)))
    }
}

#[cfg(test)]
mod toggle_tests {
    use crate::full_text::{RerankerStatus, Service};
    use crate::search_view::SearchView;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Entity, Focusable as _, Role, TestAppContext, VisualTestContext,
    };

    #[gpui_kit::test]
    fn cache_switch_uses_the_existing_policy_save_not_clear_confirmation(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let service = Service::open(
            root.path().to_owned(),
            "test-app".into(),
            vec!["test-board".into()],
        )
        .unwrap();
        let saved = service.clone();
        cx.update(gpui_kit::init);
        let (view, cx) = cx.add_window_view(|window, cx| {
            SearchView::new(
                service,
                vec![("test-board".into(), "Test board".into())],
                "test-board".into(),
                window,
                cx,
            )
        });
        cx.run_until_parked();
        let before = saved.policy("test-board").unwrap().enabled;
        view.update(cx, |view, cx| {
            assert_eq!(view.editors.len(), 1);
            view.managing = true;
            cx.notify();
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find(("source-toggle", 0usize)).label(),
                Some("Cache Test board")
            );
            window.click(("source-toggle", 0usize), cx);
        });
        cx.run_until_parked();
        assert_eq!(saved.policy("test-board").unwrap().enabled, !before);
        view.read_with(cx, |view, _| {
            assert!(view.confirm_clear.is_none());
            assert!(view.error.is_none());
        });
    }

    fn rerank_window(
        cx: &mut TestAppContext,
    ) -> (tempfile::TempDir, Entity<SearchView>, VisualTestContext) {
        let root = tempfile::tempdir().unwrap();
        let service = Service::open(
            root.path().to_owned(),
            "test-app".into(),
            vec!["test-board".into()],
        )
        .unwrap();
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(Default::default(), cx, |window, cx| {
                cx.new(|cx| {
                    SearchView::new(
                        service,
                        vec![("test-board".into(), "Test board".into())],
                        "test-board".into(),
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        let cx = VisualTestContext::from_window(handle, cx);
        cx.run_until_parked();
        (root, view, cx)
    }

    #[gpui_kit::test]
    fn rerank_button_requires_an_installed_model_and_submits_the_existing_query(
        cx: &mut TestAppContext,
    ) {
        let (_root, view, mut cx) = rerank_window(cx);
        let cx = &mut cx;
        cx.update(|window, cx| {
            window.render_frame(cx);
            let rerank = window.find("search-rerank");
            assert_eq!(rerank.role(), Some(Role::Button));
            assert_eq!(rerank.label(), Some("Local rerank"));
            assert_eq!(rerank.checked(), Some(false));
            window.click("search-rerank", cx);
            assert!(!view.read(cx).rerank);
            assert!(view.read(cx).error.is_none());
        });
        view.update(cx, |view, cx| {
            assert!(!view.rerank);
            view.model = Some(RerankerStatus {
                installed: true,
                name: "Test-only local model".into(),
                download_bytes: 0,
                message: String::new(),
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("search-rerank", cx);
            assert_eq!(window.find("search-rerank").role(), Some(Role::Button));
            assert_eq!(window.find("search-rerank").checked(), Some(true));
            view.read(cx)
                .input
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
            for _ in 0..8 {
                if window.find("search-rerank").focused() == Some(true) {
                    break;
                }
                window.press("tab", cx);
            }
            assert_eq!(window.find("search-rerank").focused(), Some(true));
            window.press("space", cx);
            assert_eq!(window.find("search-rerank").checked(), Some(false));
            assert!(!view.read(cx).rerank);
            window.press("enter", cx);
            assert_eq!(window.find("search-rerank").checked(), Some(true));
        });
        view.read_with(cx, |view, _| {
            assert!(view.rerank);
            assert!(
                view.error.is_some(),
                "empty query must still use the existing validation"
            );
            assert!(!view.installing);
        });
    }

    #[gpui_kit::test]
    fn rerank_button_rejects_activation_after_the_model_becomes_uninstalled(
        cx: &mut TestAppContext,
    ) {
        let (_root, view, mut cx) = rerank_window(cx);
        view.update(&mut cx, |view, cx| {
            view.model = Some(RerankerStatus {
                installed: true,
                name: "Test-only local model".into(),
                download_bytes: 0,
                message: String::new(),
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            view.read(cx)
                .input
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
            for _ in 0..8 {
                window.press("tab", cx);
                if window.find("search-rerank").focused() == Some(true) {
                    break;
                }
            }
            assert_eq!(window.find("search-rerank").focused(), Some(true));
        });
        let generation = view.read_with(&cx, |view, _| view.generation);
        view.update(&mut cx, |view, cx| {
            view.model.as_mut().unwrap().installed = false;
            cx.notify();
        });
        cx.update(|window, cx| {
            window.click("search-rerank", cx);
            window.press("space", cx);
            window.press("enter", cx);
            assert_eq!(window.find("search-rerank").checked(), Some(false));
            assert!(!view.read(cx).rerank);
            assert_eq!(view.read(cx).generation, generation);
            assert!(view.read(cx).error.is_none());
            assert!(!view.read(cx).installing);
        });
    }
}
