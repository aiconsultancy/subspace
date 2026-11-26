//! Check real board declarations rather than test-only shell registrations.

use subspace_framework::dashboard::settings::ConnectionField as Field;

#[test]
fn every_form_is_declared_by_its_board_with_exact_field_order_and_copy() {
    let boards = super::board_registrations();
    let fields = |id| {
        boards
            .iter()
            .find(|board| board.id == id)
            .unwrap()
            .connection_settings()
            .fields()
    };
    for id in ["github", "actions"] {
        assert_eq!(fields(id), &[Field::GitHubCredentials, Field::Repositories]);
    }
    #[cfg(feature = "copilot")]
    assert_eq!(
        fields("github-copilot-agents"),
        &[
            Field::GitHubCredentials,
            Field::Repositories,
            Field::Note(
                "Configure task filters, alerts and agent behavior in the Copilot Agents dashboard."
            )
        ]
    );
}

#[test]
#[cfg(feature = "copilot")]
fn default_off_boards_keep_their_fields() {
    let boards = super::board_registrations();
    assert!(
        !subspace_descriptor::subspace_descriptor()
            .boards
            .iter()
            .find(|board| board.id == "github-copilot-agents")
            .unwrap()
            .default_enabled
    );
    assert!(
        !boards
            .iter()
            .find(|board| board.id == "github-copilot-agents")
            .unwrap()
            .connection_settings()
            .fields()
            .is_empty(),
        "default-off boards retain their fields"
    );
}

#[test]
#[cfg(not(feature = "copilot"))]
fn a_build_without_copilot_has_no_space_or_settings() {
    let id = "github-copilot-agents";
    assert!(
        super::board_registrations()
            .iter()
            .all(|board| board.id != id)
    );
    assert!(
        subspace_descriptor::subspace_descriptor()
            .boards
            .iter()
            .all(|board| board.id != id)
    );
}
