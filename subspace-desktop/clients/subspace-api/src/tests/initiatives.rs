use subspace_framework::client_test_support::Mock;

use super::client;

#[test]
fn reads_the_current_caller_s_initiatives_and_preserves_channel_text() {
    let mock = Mock::new(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/api/v1/me/initiatives");
        (
            200,
            r#"{"items":[{"id":"dd42ef64-09f4-4082-9d22-714f108ad0cb","name":"Fixture","channels":" #a,   #B\n"}]}"#.into(),
        )
    });
    let entries = client(&mock).member_initiatives().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "dd42ef64-09f4-4082-9d22-714f108ad0cb");
    assert_eq!(entries[0].name, "Fixture");
    assert_eq!(entries[0].channels, " #a,   #B\n");
}
