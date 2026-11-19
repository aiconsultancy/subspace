use super::{access, failure};
use crate::{listing::Failure, scope::Access};
use subspace_api_client::{
    hub::{HubStatus, ReadError},
    model::Me,
};
#[test]
fn connection_uses_me_and_edge_failures_remove_access() {
    let me: Me = serde_json::from_value(
        serde_json::json!({"userId":"fixture-me","kind":"human","displayName":"Ada Fixture"}),
    )
    .unwrap();
    assert_eq!(
        access(&HubStatus::Ready, Some(&me)),
        Access::Ready {
            display_name: "Ada Fixture".into()
        }
    );
    assert_eq!(
        access(&HubStatus::Error(ReadError::SignInNeeded), Some(&me)),
        Access::Failed(Failure::Edge)
    );
    assert_eq!(failure(&ReadError::Network), Failure::Network);
    assert_eq!(failure(&ReadError::Forbidden), Failure::Forbidden);
}
