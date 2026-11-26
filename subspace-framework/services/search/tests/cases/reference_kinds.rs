use super::{document, enable, setup};
use subspace_search::{
    IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch, RelatedQuery,
    register_reference_kind,
};

#[test]
fn registered_kind_is_stored_and_matched_while_others_stay_refused() {
    assert!(ReferenceIdentity::parse("example-ref:42").is_err());
    assert!(register_reference_kind("Example Ref").is_err());
    register_reference_kind("example-ref").unwrap();
    register_reference_kind("example-ref").unwrap();
    let identity = ReferenceIdentity::of_kind("example-ref", &[" 42 "]).unwrap();
    assert_eq!(identity.as_str(), "example-ref:42");
    assert_eq!(
        ReferenceIdentity::parse("example-ref:42").unwrap(),
        identity
    );
    assert!(ReferenceIdentity::parse("example-other:42").is_err());

    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service.upsert(&scope, &[document("a", "needle")]).unwrap();
    let metadata = IndexMetadata {
        project: None,
        references: vec![Reference {
            identity: identity.clone(),
            relation_type: "work-item".into(),
            matched: ReferenceMatch::Exact,
        }],
    };
    service.index_metadata(&scope, "a", &metadata).unwrap();
    let related = service
        .related_items(
            &identity,
            &RelatedQuery {
                limit: 20,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(related.len(), 1);
}
