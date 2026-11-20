//! The NETT backend: the same client operations sent to NETT's
//! organisation-scoped routes and read back as the desktop's models.
//! Requests go to `/api/v1/orgs/{orgId}/…` with `x-org-id`, and the bearer
//! only ever reaches the configured API origin.

mod chat;
pub(crate) mod flow;
mod lists;
pub(crate) mod media;
pub(crate) mod query;
mod reads;
pub mod scope;
pub(crate) mod transfer;
pub(crate) mod wire;

pub use lists::{TodoItem, TodoList};
pub use scope::{Organisation, Snapshot, Status, snapshot};

/// Remembers organisation choices between runs, in a file beside the app's
/// state. The product calls this once at startup.
pub fn remember_organisations() {
    scope::saved::enable();
}

/// Chooses the NETT organisation the app works in and remembers it for this
/// API origin and user. Every client, hub and read made for the previous
/// organisation is retired, and a pause NETT asked for ends. Returns false
/// for an organisation the account cannot use.
pub fn select_organisation(id: &str) -> bool {
    if !scope::select(id) {
        return false;
    }
    media::forget_grants();
    // The person chose where to work: a pause NETT asked for ends now.
    flow::resume(crate::native::account_generation().unwrap_or_default());
    if let (Some(origin), Some(user)) = (crate::native::origin(), scope::user()) {
        scope::saved::store(origin.as_str(), &user, id);
    }
    true
}

/// The organisation path prefix and `x-org-id` value an organisation route
/// needs, from its URL path `/api/v1/orgs/{orgId}/…`.
pub(crate) fn route_organisation(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/v1/orgs/")?;
    let (org, tail) = rest.split_once('/')?;
    (!tail.is_empty() && uuid::Uuid::parse_str(org).is_ok()).then_some(org)
}

/// Adds `x-org-id` for the organisation the request's path names. NETT
/// requires it for file grants and checks it matches the route elsewhere.
pub(crate) fn add_organisation_header(request: &mut ureq::http::Request<ureq::SendBody>) {
    let Some(org) = route_organisation(request.uri().path()).map(str::to_owned) else {
        return;
    };
    if let Ok(value) = org.parse() {
        request.headers_mut().insert("x-org-id", value);
    }
}

#[cfg(test)]
mod tests {
    use super::route_organisation;

    #[test]
    fn only_organisation_routes_carry_an_organisation_header() {
        let org = "00000000-0000-0000-0000-000000000001";
        assert_eq!(
            route_organisation(&format!("/api/v1/orgs/{org}/issues")),
            Some(org)
        );
        assert_eq!(route_organisation("/api/v1/auth/context"), None);
        assert_eq!(route_organisation("/api/v1/orgs/not-a-uuid/issues"), None);
        assert_eq!(route_organisation(&format!("/api/v1/orgs/{org}/")), None);
    }
}
