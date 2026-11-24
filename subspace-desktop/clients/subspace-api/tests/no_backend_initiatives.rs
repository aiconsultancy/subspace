//! A build with no API backend keeps its local behaviour: the shared
//! initiative catalogue reads nothing, selects nothing, remembers nothing
//! and never says the person belongs to no initiative. Its own process,
//! because the backend is chosen once.

use gpui_kit::TestAppContext;
use subspace_api_client::initiatives::Catalog;
use subspace_framework::dashboard::prefs;

#[gpui_kit::test]
fn a_build_without_a_backend_selects_and_remembers_no_initiative(cx: &mut TestAppContext) {
    subspace_api_client::configure_no_backend().unwrap();
    cx.update(|cx| prefs::init_file(None, cx));
    let catalog = cx.update(Catalog::shared);
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    catalog.update(cx, |catalog, cx| {
        assert!(!catalog.memberships_known() && !catalog.has_memberships());
        assert!(
            !catalog.no_memberships(),
            "nothing closes Channels in a build without an API"
        );
        assert!(catalog.selected().is_none());
        assert!(catalog.menu().options.is_empty());
        catalog.pick("local", cx);
        assert!(catalog.selected().is_none());
    });
    cx.update(|cx| assert_eq!(prefs::get(cx, "initiatives.selected"), None));
}
