use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Window};

use super::{SearchView, SearchViewEvent, SourceEditor, policy};
use crate::full_text::{Policy, Service};

impl SearchView {
    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.invalidate(cx);
        self.notice = None;
        let query = match policy::query(
            &self.input.read(cx).value(),
            &self.current,
            self.all,
            self.rerank,
        ) {
            Ok(query) => query,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let generation = self.generation;
        let service = self.service.clone();
        self.loading = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { service.search(&query) })
                .await;
            _ = this.update_in(cx, |this, _, cx| {
                if generation != this.generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(response) => {
                        this.selected = None;
                        this.response = Some(response);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Refresh after external policy/account changes. Invalidates visible results immediately.
    pub fn refresh_sources(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.invalidate(cx);
        self.source_generation = self.source_generation.wrapping_add(1);
        let generation = self.source_generation;
        let service = self.service.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let sources = service.sources()?;
                    let policies = sources
                        .iter()
                        .map(|source| {
                            service
                                .policy(&source.board)
                                .map(|policy| (source.board.clone(), policy))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok::<_, String>((sources, policies, service.reranker_status()))
                })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                if generation != this.source_generation {
                    return;
                }
                match result {
                    Ok((sources, policies, model)) => {
                        if !policy::participates(&sources, &this.current) {
                            this.all = true;
                        }
                        this.sources = sources;
                        this.model = model;
                        this.editors = policies
                            .into_iter()
                            .map(|(board, policy)| source_editor(board, policy, window, cx))
                            .collect();
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_result(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(hit) = self
            .response
            .as_ref()
            .and_then(|r| r.hits.get(index))
            .cloned()
        else {
            return;
        };
        let service = self.service.clone();
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let target = hit.target.clone();
            let result = cx.background_spawn(async move { service.read(&target) }).await;
            _ = this.update_in(cx, |this, _, cx| {
                if generation != this.generation { return; }
                match result {
                    Ok(Some(document)) => cx.emit(SearchViewEvent::Open(Box::new(crate::full_text::Hit { document, ..hit }))),
                    Ok(None) => {
                        this.invalidate(cx);
                        this.error = Some("This cached item is no longer available. Search again to refresh results.".into());
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn save_source(
        &mut self,
        index: usize,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editors.get(index) else {
            return;
        };
        let policy = if !enabled && editor.policy.enabled {
            Ok(Policy {
                enabled: false,
                ..editor.policy.clone()
            })
        } else {
            policy::draft_policy(
                enabled,
                &editor.days.read(cx).value(),
                &editor.gigabytes.read(cx).value(),
            )
        };
        match policy {
            Ok(policy) => {
                let board = editor.board.clone();
                self.mutate(window, cx, move |service| {
                    service.set_policy(&board, policy)
                });
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub(super) fn clear_source(
        &mut self,
        board: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_clear = None;
        self.mutate(window, cx, move |service| service.clear(&board));
    }

    fn mutate(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(Service) -> Result<(), String> + Send + 'static,
    ) {
        if self.busy {
            return;
        }
        self.invalidate(cx);
        self.source_generation = self.source_generation.wrapping_add(1);
        let generation = self.source_generation;
        self.busy = true;
        self.notice = None;
        let service = self.service.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { operation(service) }).await;
            _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if generation != this.source_generation {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(()) => {
                        this.notice = Some(
                            "Source settings updated. Search again to see current content.".into(),
                        );
                        this.refresh_sources(window, cx);
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn install_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.installing {
            return;
        }
        self.installing = true;
        self.error = None;
        let service = self.service.clone();
        cx.spawn_in(window, async move |this, cx| {
            let (result, model) = cx
                .background_spawn(
                    async move { (service.install_reranker(), service.reranker_status()) },
                )
                .await;
            _ = this.update_in(cx, |this, _, cx| {
                this.installing = false;
                this.model = model;
                if let Err(error) = result {
                    this.error = Some(error);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn source_editor(
    board: String,
    policy: Policy,
    window: &mut Window,
    cx: &mut Context<SearchView>,
) -> SourceEditor {
    let days = cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(policy.retention_days.to_string(), window, cx);
        input
    });
    let gigabytes = cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(
            (policy.max_bytes as f64 / 1_000_000_000.).to_string(),
            window,
            cx,
        );
        input
    });
    SourceEditor {
        board,
        policy,
        days,
        gigabytes,
    }
}
