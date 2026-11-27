//! Opening a linked issue in the Space that answers the `issues` contract,
//! as Channels and web Spaces do: the Space asks for the current scope,
//! binds the issue's address to it, and the shell opens that address in the
//! answering Space through `Dashboard::open_link`. This Space never names
//! the Issues Space or depends on its crate.

use gpui_kit::Context;
use subspace_framework::contracts::decisions::OpenDecision;
use subspace_framework::contracts::issues::{CurrentScope, IssueScope};
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, MessageKey};
use subspace_framework::dashboard::{BoardNavigation, DashboardEvent, Toast};

use crate::{Decisions, ID, page};

pub(crate) const CONTRACTS: BoardContracts = BoardContracts::new(&[
    Declaration::requests::<CurrentScope>(),
    Declaration::responds::<OpenDecision>(),
]);

/// The address the answering Space opens for `identifier`, bound to its
/// profile, or none when the scope does not recognise the identifier.
pub(crate) fn address(scope: &IssueScope, identifier: &str) -> Option<String> {
    let url = format!(
        "{}/v1/issues/{identifier}",
        scope.api_origin.trim_end_matches('/')
    );
    scope.navigation_url(&url)
}

/// Why an issue did not open.
pub(crate) fn unavailable(identifier: &str) -> String {
    format!("{identifier} can't open here. Turn on Issues in Settings and sign in, then try again.")
}

/// Shows issue `identifier` in the Space that answers for issues, or says
/// why it can't. Nothing opens once the decision it was listed under was
/// read for an account that is no longer signed in.
pub(crate) fn open(identifier: &str, decisions: &mut Decisions, cx: &mut Context<Decisions>) {
    if decisions.follow_account(cx) {
        return;
    }
    let scope = contracts::request(ID, &CurrentScope, cx).ok().flatten();
    let event = match scope.and_then(|scope| address(&scope, identifier)) {
        Some(url) => DashboardEvent::Navigate(BoardNavigation {
            responder: MessageKey::request::<CurrentScope>(),
            url: url.into(),
            // Back in Issues returns to the open decision, or to the
            // decision selected in the list.
            origin: page::issue_origin(decisions),
        }),
        None => DashboardEvent::Toast(Toast::info(unavailable(identifier))),
    };
    cx.emit(event);
}

#[cfg(test)]
mod tests {
    use subspace_framework::contracts::issues::IssueScope;

    use super::{address, unavailable};

    fn scope() -> IssueScope {
        IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:3000/api")
            .with_project_keys(["NAV", "WARP"])
    }

    #[test]
    fn a_linked_issue_opens_at_its_address_in_the_answering_scope() {
        assert_eq!(
            address(&scope(), "NAV-2").as_deref(),
            Some("http://127.0.0.1:3000/api/v1/issues/NAV-2?profile=subspace-api")
        );
    }

    #[test]
    fn an_issue_outside_the_scope_does_not_open() {
        assert_eq!(address(&scope(), "OPS-1"), None);
        assert!(unavailable("OPS-1").starts_with("OPS-1 can't open here."));
    }
}
