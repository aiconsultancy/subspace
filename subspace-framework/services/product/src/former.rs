//! A Space's former ids: the ids it had in earlier versions of the product.
//! Saved settings and state under a former id are read as the Space's own
//! (`subspace_core::former_ids`), so a former id must name nothing else.

use std::collections::BTreeSet;

use crate::descriptor::{ValidatedBoard, ValidatedProduct};
use crate::error::ProductError;
use crate::mcp::MCP_APPLICATION_GROUP;

/// Names the Framework saves its own data under, which no Space ever had:
/// the state file's top-level namespaces that are not a Space's (`app.*`,
/// `board.*`, `notifications.*` and the rest), the config directory's
/// folders that are not a Space's settings folder (`themes`), and the
/// Application's shared MCP group in `mcp.groups` ([`MCP_APPLICATION_GROUP`]).
/// The reader maps `<former id>.*` state keys, `mcp.groups.<former id>` and
/// `<config dir>/<former id>/`, so a former id with one of these names would
/// hand the Framework's data to a Space.
pub const SAVED_NAMES: &[&str] = &[
    "alerts",
    "app",
    MCP_APPLICATION_GROUP,
    "appearance",
    "board",
    "chat",
    "notifications",
    "palette",
    "terminal",
    "themes",
];

/// What a product's own Spaces use an id for, which no configured Space may
/// then take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardIdUse {
    /// A Space's id.
    Id,
    /// A Space's former id, whose saved settings are read as that Space's.
    FormerId,
}

impl ValidatedProduct {
    /// How the product's own Spaces use `id`, if at all.
    pub fn board_id_use(&self, id: &str) -> Option<BoardIdUse> {
        let boards = || self.boards.iter();
        if boards().any(|board| board.id.as_str() == id) {
            return Some(BoardIdUse::Id);
        }
        boards()
            .flat_map(|board| &board.former_ids)
            .any(|former| former.as_str() == id)
            .then_some(BoardIdUse::FormerId)
    }
}

/// Refuses a former id that is a Space's id in the product, one declared
/// twice, or one of the [`SAVED_NAMES`], comparing ASCII case-insensitively:
/// a former id names a settings folder, and macOS and Windows match folder
/// names without regard to case. Refuses any former id of a Space whose own
/// id is one of the [`SAVED_NAMES`]: what was saved under it would land
/// among the Framework's own data, or, for the Application's MCP group,
/// could not land at all.
pub(crate) fn validate(boards: &[ValidatedBoard]) -> Result<(), ProductError> {
    let saved = |id: &str| SAVED_NAMES.contains(&id.to_ascii_lowercase().as_str());
    if let Some(board) = boards
        .iter()
        .find(|board| !board.former_ids.is_empty() && saved(board.id.as_str()))
    {
        let name = board.id.as_str().to_owned();
        return Err(ProductError::ReservedRenameTarget { name });
    }
    let ids: BTreeSet<String> = boards
        .iter()
        .map(|board| board.id.as_str().to_ascii_lowercase())
        .collect();
    let mut seen = BTreeSet::new();
    for former in boards.iter().flat_map(|board| &board.former_ids) {
        let name = former.as_str().to_owned();
        let folded = name.to_ascii_lowercase();
        if ids.contains(&folded) {
            return Err(ProductError::FormerIdInUse { name });
        }
        if saved(&name) {
            return Err(ProductError::ReservedFormerId { name });
        }
        if !seen.insert(folded) {
            return Err(ProductError::DuplicateFormerId { name });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BoardIdUse, SAVED_NAMES};
    use crate::{BoardDescriptor, ProductError, fixture, validate_product};

    fn with_former_ids(boards: &[(&str, &[&str])]) -> Result<(), ProductError> {
        let mut descriptor = fixture::descriptor();
        descriptor.boards = boards
            .iter()
            .map(|(id, former)| BoardDescriptor {
                id: (*id).into(),
                aliases: Vec::new(),
                former_ids: former.iter().map(|name| (*name).to_owned()).collect(),
                default_enabled: true,
            })
            .collect();
        validate_product(&descriptor).map(|_| ())
    }

    #[test]
    fn a_product_says_how_its_spaces_use_an_id() {
        let mut descriptor = fixture::descriptor();
        descriptor.boards[0].former_ids = vec!["example-old".into()];
        let product = validate_product(&descriptor).unwrap();
        let id = product.boards[0].id.as_str().to_owned();
        assert_eq!(product.board_id_use(&id), Some(BoardIdUse::Id));
        assert_eq!(
            product.board_id_use("example-old"),
            Some(BoardIdUse::FormerId)
        );
        assert_eq!(product.board_id_use("example-other"), None);
    }

    #[test]
    fn a_former_id_is_kept_on_the_validated_space() {
        let mut descriptor = fixture::descriptor();
        descriptor.boards[0].former_ids = vec!["example-old".into()];
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.boards[0].former_ids[0].as_str(), "example-old");
        assert!(product.board_for_command("example-old").is_none());
    }

    #[test]
    fn a_former_id_must_be_a_safe_identity() {
        assert_eq!(
            with_former_ids(&[("example-new", &["../old"])]),
            Err(ProductError::UnsafeIdentity {
                name: "../old".into()
            })
        );
    }

    #[test]
    fn a_former_id_never_names_a_space_of_the_product() {
        for boards in [
            &[("example-new", &["example-new"][..])][..],
            &[("example-new", &["example-two"]), ("example-two", &[])],
        ] {
            let error = with_former_ids(boards).unwrap_err();
            assert!(
                matches!(error, ProductError::FormerIdInUse { .. }),
                "{error}"
            );
        }
    }

    #[test]
    fn a_former_id_belongs_to_one_space_once() {
        let name = || "example-old".to_owned();
        assert_eq!(
            with_former_ids(&[("example-new", &["example-old", "example-old"])]),
            Err(ProductError::DuplicateIdentity { name: name() })
        );
        assert_eq!(
            with_former_ids(&[
                ("example-new", &["example-old"]),
                ("example-two", &["example-old"]),
            ]),
            Err(ProductError::DuplicateFormerId { name: name() })
        );
    }

    #[test]
    fn a_name_the_framework_saves_under_is_never_a_former_id() {
        for name in SAVED_NAMES {
            assert_eq!(
                with_former_ids(&[("example-new", &[name])]),
                Err(ProductError::ReservedFormerId {
                    name: (*name).to_owned()
                }),
                "{name}"
            );
        }
        assert_eq!(
            with_former_ids(&[("example-new", &["example-old"])]),
            Ok(())
        );
    }

    /// Former ids name folders, which macOS and Windows match without case.
    #[test]
    fn former_ids_collide_without_regard_to_ascii_case() {
        let error = with_former_ids(&[("example-new", &["example-old"]), ("Example-Old", &[])]);
        assert_eq!(
            error,
            Err(ProductError::FormerIdInUse {
                name: "example-old".into()
            })
        );
        for boards in [
            &[("example-new", &["example-old", "EXAMPLE-OLD"][..])][..],
            &[
                ("example-new", &["example-old"]),
                ("example-two", &["Example-Old"]),
            ],
        ] {
            let error = with_former_ids(boards).unwrap_err();
            assert!(
                matches!(error, ProductError::DuplicateFormerId { .. }),
                "{error}"
            );
        }
        assert_eq!(
            with_former_ids(&[("example-new", &["Themes"])]),
            Err(ProductError::ReservedFormerId {
                name: "Themes".into()
            })
        );
    }

    /// Data saved under a former id would land in the Framework's own
    /// namespace, and `mcp.groups.application` never takes a moved choice.
    #[test]
    fn a_space_under_a_name_the_framework_saves_under_has_no_former_ids() {
        for id in [crate::MCP_APPLICATION_GROUP, "App"] {
            assert_eq!(
                with_former_ids(&[(id, &["example-old"])]),
                Err(ProductError::ReservedRenameTarget { name: id.into() }),
                "{id}"
            );
        }
        assert_eq!(
            with_former_ids(&[(crate::MCP_APPLICATION_GROUP, &[])]),
            Ok(())
        );
    }

    #[test]
    fn the_application_mcp_group_is_never_a_former_id() {
        assert_eq!(
            with_former_ids(&[("example-new", &[crate::MCP_APPLICATION_GROUP])]),
            Err(ProductError::ReservedFormerId {
                name: crate::MCP_APPLICATION_GROUP.to_owned()
            })
        );
    }
}
