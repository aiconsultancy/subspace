//! Board-agnostic helpers for persistent-search adapters.

use crate::full_text;
use crate::prefetch::Source;
use gpui_kit::{AppContext as _, Context, Task};

/// Credential-bound identity data safe to retain outside a client.
pub struct Identity {
    pub endpoint: String,
    pub account: String,
    pub key: String,
}

/// An enabled source and the identity used to activate it.
pub struct Binding {
    pub source: Source,
    pub scope: full_text::Scope,
    pub identity: String,
}

/// A binding together with board-owned fetch dependencies.
pub struct Run<C, K> {
    pub source: Source,
    pub scope: full_text::Scope,
    pub identity: String,
    pub client: C,
    pub cache: K,
}

/// Stable source names supplied to [`bind_run`].
pub struct SourceId<'a> {
    pub app: &'a str,
    pub board: &'a str,
}

/// Activates an enabled board source with a verified, non-secret identity.
pub fn bind(
    service: full_text::Service,
    app: &str,
    board: &str,
    verify: impl FnOnce() -> Result<Identity, String>,
) -> Result<Option<Binding>, String> {
    let mut identity = None;
    let source = Source::bind(service, board, || {
        let verified = verify()?;
        let scope = scope(app, board, &verified.endpoint, &verified.account)?;
        identity = Some((scope.clone(), verified.key));
        Ok(scope)
    })?;
    match (source, identity) {
        (Some(source), Some((scope, identity))) => Ok(Some(Binding {
            source,
            scope,
            identity,
        })),
        (None, None) => Ok(None),
        _ => Err("Search source activation did not retain its identity".into()),
    }
}

/// Binds a source and carries its client and cache into a background run.
pub fn bind_run<C, K>(
    service: full_text::Service,
    id: SourceId<'_>,
    client: C,
    cache: K,
    verify: impl FnOnce(&C) -> Result<Identity, String>,
) -> Result<Option<Run<C, K>>, String> {
    let binding = bind(service, id.app, id.board, || verify(&client))?;
    Ok(binding.map(|binding| Run {
        source: binding.source,
        scope: binding.scope,
        identity: binding.identity,
        client,
        cache,
    }))
}

/// What one board's `schedule` already has in hand: its client, cache and
/// loaded items, plus the closure that turns the bound client into a
/// non-secret identity.
pub struct Bind<C, K, T> {
    pub client: C,
    pub cache: K,
    pub loaded: Vec<T>,
    pub verify: Box<dyn FnOnce(&C) -> Result<Identity, String> + Send>,
}

/// Binds a board's source on the background executor, then applies `start`
/// with the bound run and loaded items on the UI thread. Call once from
/// `schedule`, after cancelling any previous run; store the returned task so
/// dropping it cancels the bind.
pub fn schedule<B, T, C, K>(
    cx: &mut Context<B>,
    service: full_text::Service,
    board_name: &str,
    bind: Bind<C, K, T>,
    start: impl FnOnce(&mut B, Run<C, K>, Vec<T>, full_text::Service, &mut Context<B>) + Send + 'static,
) -> Task<()>
where
    B: 'static,
    T: Send + 'static,
    C: Send + 'static,
    K: Send + 'static,
{
    let Bind {
        client,
        cache,
        loaded,
        verify,
    } = bind;
    let app = service.app_id().to_owned();
    let board_name = board_name.to_owned();
    let service_for_index = service.clone();
    cx.spawn(async move |this, cx| {
        let bound = cx
            .background_spawn(async move {
                bind_run(
                    service,
                    SourceId {
                        app: &app,
                        board: &board_name,
                    },
                    client,
                    cache,
                    verify,
                )
                .ok()?
            })
            .await;
        let Some(bound) = bound else {
            return;
        };
        let _ = this.update(cx, |board_state, cx| {
            start(board_state, bound, loaded, service_for_index, cx)
        });
    })
}

/// Builds a complete search scope from a verified identity.
pub fn scope(
    app: &str,
    board: &str,
    endpoint: &str,
    account: &str,
) -> Result<full_text::Scope, String> {
    if app.trim().is_empty()
        || board.trim().is_empty()
        || endpoint.trim().is_empty()
        || account.trim().is_empty()
    {
        return Err("Search identity is incomplete".into());
    }
    Ok(full_text::Scope {
        app: app.into(),
        board: board.into(),
        endpoint: endpoint.into(),
        tenant: String::new(),
        account: account.into(),
    })
}

/// Returns a loaded work list only when it contains something to schedule.
pub fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
    (!items.is_empty()).then_some(items)
}

/// Where a run's fetched documents are indexed.
// qual:api
#[derive(Clone)]
pub struct Indexing {
    pub service: full_text::Service,
    pub scope: full_text::Scope,
}

/// Whether a search write's refusal means a verified scope is not (or no
/// longer) the board's active search account, never a real write failure.
/// A refresh maps this to "not indexed", not an error.
// qual:api
pub fn inactive_scope(error: &str) -> bool {
    error == "Search scope is inactive or caching is disabled"
}

/// Bounds indexed text with an explicit truncation marker.
pub fn bounded_text(text: String, limit: usize, marker: &str) -> String {
    crate::context_text::cap_to(text, limit, marker)
}

#[cfg(test)]
mod tests {
    use super::{Identity, bind, bounded_text, inactive_scope, scope};
    use crate::full_text::{Policy, Service};

    #[test]
    fn disabled_source_does_not_verify_identity() {
        let root = std::env::temp_dir().join(format!(
            "subspace-search-adapter-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let service = Service::open(root.clone(), "fixture".into(), vec!["board".into()]).unwrap();
        let mut verified = false;
        let binding = bind(service.clone(), "fixture", "board", || {
            verified = true;
            Ok(identity())
        })
        .unwrap();
        assert!(binding.is_none());
        assert!(!verified);

        service
            .set_policy(
                "board",
                Policy {
                    enabled: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            bind(service, "fixture", "board", || Ok(identity()))
                .unwrap()
                .is_some()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scope_and_text_are_validated_and_bounded() {
        assert!(scope("app", "board", "https://example.com", "account").is_ok());
        assert!(scope("app", "board", "", "account").is_err());
        assert_eq!(bounded_text("ééé".into(), 5, "!"), "éé!");
    }

    #[test]
    fn write_refusal_for_an_inactive_scope_is_distinguished_from_a_real_failure() {
        assert!(inactive_scope(
            "Search scope is inactive or caching is disabled"
        ));
        assert!(!inactive_scope("Search storage operation failed"));
    }

    fn identity() -> Identity {
        Identity {
            endpoint: "https://example.com".into(),
            account: "config:token".into(),
            key: "config:token@https://example.com".into(),
        }
    }
}
