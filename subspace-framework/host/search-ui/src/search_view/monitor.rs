use std::{collections::BTreeSet, time::Duration};

use gpui_kit::{AppContext as _, Context, Window};

use super::SearchView;
use crate::full_text::{SourceStatus, Target};

const SOURCE_REFRESH_INTERVAL: Duration = Duration::from_secs(3);

impl SearchView {
    pub(super) fn monitor_sources(&self, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.service.clone();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(SOURCE_REFRESH_INTERVAL)
                    .await;
                let Ok((generation, source_generation, targets)) = this
                    .update_in(cx, |this, _, _| {
                        (this.generation, this.source_generation, this.scope_probes())
                    })
                else {
                    break;
                };
                let service = service.clone();
                let result = cx
                    .background_spawn(async move {
                        let sources = service.sources()?;
                        // SourceStatus exposes account, not tenant/endpoint. Reading one target
                        // per visible board detects those scope changes without indexing work.
                        let valid = targets
                            .iter()
                            .all(|target| matches!(service.read(target), Ok(Some(_))));
                        Ok::<_, String>((sources, valid))
                    })
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        if generation != this.generation
                            || source_generation != this.source_generation
                        {
                            return;
                        }
                        this.accept_source_check(result, window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn scope_probes(&self) -> Vec<Target> {
        let mut seen = BTreeSet::new();
        self.response
            .iter()
            .flat_map(|response| &response.hits)
            .filter(|hit| seen.insert(hit.target.scope.board.clone()))
            .map(|hit| hit.target.clone())
            .collect()
    }

    fn accept_source_check(
        &mut self,
        result: Result<(Vec<SourceStatus>, bool), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((sources, valid)) => {
                if identities(&sources) != identities(&self.sources) {
                    self.refresh_sources(window, cx);
                    self.notice = Some("Search sources changed. Search again to use the current account and settings.".into());
                } else if !valid {
                    self.invalidate(cx);
                    self.notice = Some(
                        "Cached results changed or expired. Search again to refresh them.".into(),
                    );
                } else if self.managing && usage(&sources) != usage(&self.sources) {
                    cx.notify();
                }
                self.sources = sources;
            }
            Err(error) if self.response.is_some() || self.loading => {
                self.invalidate(cx);
                self.error = Some(error);
            }
            Err(_) => {}
        }
    }
}

fn identities(sources: &[SourceStatus]) -> BTreeSet<(&str, bool, Option<&str>)> {
    sources
        .iter()
        .map(|source| {
            (
                source.board.as_str(),
                source.enabled,
                source.account.as_deref(),
            )
        })
        .collect()
}

fn usage(sources: &[SourceStatus]) -> BTreeSet<(&str, u64, u64, &str)> {
    sources
        .iter()
        .map(|source| {
            (
                source.board.as_str(),
                source.records,
                source.bytes,
                source.message.as_str(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::identities;
    use crate::full_text::SourceStatus;

    #[test]
    fn ordinary_cache_growth_does_not_invalidate_scope_but_account_and_policy_do() {
        let mut source = SourceStatus {
            board: "irc".into(),
            enabled: true,
            account: Some("a".into()),
            records: 1,
            bytes: 100,
            message: "Partial history".into(),
        };
        let original = [source.clone()];
        source.records = 2;
        source.bytes = 200;
        assert_eq!(identities(&original), identities(&[source.clone()]));
        source.account = Some("b".into());
        assert_ne!(identities(&original), identities(&[source.clone()]));
        source.account = Some("a".into());
        source.enabled = false;
        assert_ne!(identities(&original), identities(&[source]));
    }
}
