use super::{PackageRelease, RepositoryRef, download::redirect_allowed, newest_release, selection};
use serde_json::json;
use url::Url;

fn release(id: u64, prerelease: bool, name: &str) -> PackageRelease {
    serde_json::from_value(json!({"id":id,"tag_name":format!("v{id}"),"draft":false,"prerelease":prerelease,"published_at":format!("2026-10-{id:02}T00:00:00Z"),"assets":[{"id":id,"name":name,"size":12}]})).unwrap()
}

#[test]
fn stable_exact_named_zip_is_default_and_prereleases_are_explicit() {
    let rows = [
        release(1, false, "board.zip"),
        release(2, false, "unrelated.zip"),
        release(3, true, "board.zip"),
    ];
    assert_eq!(newest_release(&rows, "board.zip", false).unwrap().id, 1);
    assert_eq!(newest_release(&rows, "board.zip", true).unwrap().id, 3);
    assert!(newest_release(&rows, "Board.zip", false).is_none());
    assert!(newest_release(&rows, "unrelated.tar", true).is_none());
}

#[test]
fn repository_access_requires_membership_and_organization_read_permission() {
    assert!(
        selection::membership(
            &json!({"state":"pending","organization":{"login":"acme"}}),
            "acme"
        )
        .is_err()
    );
    assert!(
        selection::membership(
            &json!({"state":"active","organization":{"login":"elsewhere"}}),
            "acme"
        )
        .is_err()
    );
    let mut value = json!({"full_name":"acme/board","default_branch":"actual-main","owner":{"type":"User"},"permissions":{"pull":true},"private":false});
    assert!(selection::repository(value.clone(), "acme/board").is_err());
    value["owner"]["type"] = json!("Organization");
    value["permissions"]["pull"] = json!(false);
    assert!(selection::repository(value.clone(), "acme/board").is_err());
    value["permissions"]["pull"] = json!(true);
    assert!(selection::repository(value, "acme/board").is_ok());
}

#[test]
fn explicit_refs_preserve_namespace_and_actual_default_branch() {
    assert_eq!(
        selection::effective_ref(&RepositoryRef::DefaultBranch, "trunk").unwrap(),
        RepositoryRef::Branch("trunk".into())
    );
    assert_eq!(
        selection::expression(&RepositoryRef::Tag("main".into())).unwrap(),
        "refs/tags/main"
    );
    assert_eq!(
        selection::expression(&RepositoryRef::Branch("main".into())).unwrap(),
        "refs/heads/main"
    );
    assert!(selection::expression(&RepositoryRef::Commit("main".into())).is_err());
    for expression in [
        "main~1",
        "main^",
        "main@{1}",
        "feature//test",
        "bad.lock",
        ".hidden",
    ] {
        assert!(selection::expression(&RepositoryRef::Branch(expression.into())).is_err());
    }
}

#[test]
fn download_redirect_policy_rejects_credential_and_origin_confusion() {
    let initial = Url::parse("https://api.github.com/repos/acme/board/zipball/test").unwrap();
    for target in [
        "https://attacker.test/file",
        "https://api.github.com.attacker.test/file",
        "http://codeload.github.com/file",
        "https://dummy:fake@codeload.github.com/file",
        "https://codeload.github.com:8443/file",
    ] {
        assert!(
            redirect_allowed(&initial, &Url::parse(target).unwrap()).is_err(),
            "{target}"
        );
    }
    assert!(
        redirect_allowed(
            &initial,
            &Url::parse(
                "https://codeload.github.com/acme/board/zip/test?signature=dummy-test-value"
            )
            .unwrap()
        )
        .is_ok()
    );
}
