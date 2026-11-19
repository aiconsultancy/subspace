//! Product, board, binary and command-alias identities.

use crate::error::ProductError;

/// A validated product, board, binary, theme or command-alias name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identity(String);

impl Identity {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// ASCII alphanumeric, hyphen or underscore; first character alphanumeric.
pub fn parse_identity(value: &str) -> Result<Identity, ProductError> {
    identity_from_flag(value, identity_is_safe(value))
}

fn identity_is_safe(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => chars.all(is_identity_char),
        _ => false,
    }
}

fn is_identity_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn identity_from_flag(value: &str, safe: bool) -> Result<Identity, ProductError> {
    if safe {
        Ok(Identity(value.to_string()))
    } else {
        Err(ProductError::UnsafeIdentity {
            name: value.to_string(),
        })
    }
}

/// Reverse-DNS bundle identity with at least two safe labels.
pub fn parse_bundle_id(value: &str) -> Result<String, ProductError> {
    bundle_from_flag(value, bundle_id_is_safe(value))
}

fn bundle_id_is_safe(value: &str) -> bool {
    let mut labels = value.split('.');
    match labels.next() {
        Some(first) if identity_is_safe(first) && value.contains('.') => {
            labels.all(identity_is_safe)
        }
        _ => false,
    }
}

fn bundle_from_flag(value: &str, safe: bool) -> Result<String, ProductError> {
    if safe {
        Ok(value.to_string())
    } else {
        Err(ProductError::UnsafeBundleId {
            name: value.to_string(),
        })
    }
}

/// Display names may contain spaces. They are not path components.
pub fn parse_display_name(value: &str) -> Result<String, ProductError> {
    display_from_flag(value, display_name_is_safe(value))
}

fn display_name_is_safe(value: &str) -> bool {
    !value.is_empty() && value.trim() == value && value.chars().all(|c| !c.is_control())
}

fn display_from_flag(value: &str, safe: bool) -> Result<String, ProductError> {
    if safe {
        Ok(value.to_string())
    } else {
        Err(ProductError::UnsafeDisplayName {
            name: value.to_string(),
        })
    }
}

/// Relative asset path with no empty, `.` or `..` components.
pub fn parse_asset_path(value: &str) -> Result<String, ProductError> {
    asset_from_flag(value, asset_path_is_safe(value))
}

fn asset_path_is_safe(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && !has_windows_drive_prefix(value)
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn asset_from_flag(value: &str, safe: bool) -> Result<String, ProductError> {
    if safe {
        Ok(value.to_string())
    } else {
        Err(ProductError::UnsafeAssetPath {
            name: value.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_asset_path, parse_bundle_id, parse_display_name, parse_identity};
    use crate::error::ProductError;

    #[test]
    fn parse_identity_accepts_product_and_board_names() {
        let id = parse_identity("board-lab").unwrap();
        assert_eq!(id.as_str(), "board-lab");
    }

    #[test]
    fn parse_identity_rejects_unsafe_values() {
        for name in [
            "",
            "board lab",
            "../etc",
            "foo/bar",
            r"foo\bar",
            "-lead",
            "_x",
        ] {
            assert_eq!(
                parse_identity(name).unwrap_err(),
                ProductError::UnsafeIdentity {
                    name: name.to_string()
                }
            );
        }
    }

    #[test]
    fn parse_bundle_id_requires_two_labels() {
        assert_eq!(
            parse_bundle_id("com.example.subspace").unwrap(),
            "com.example.subspace"
        );
        assert_eq!(
            parse_bundle_id("subspace").unwrap_err(),
            ProductError::UnsafeBundleId {
                name: "subspace".into()
            }
        );
    }

    #[test]
    fn parse_display_name_rejects_blank_or_control_text() {
        assert_eq!(parse_display_name("Board Lab").unwrap(), "Board Lab");
        assert_eq!(
            parse_display_name(" Subspace").unwrap_err(),
            ProductError::UnsafeDisplayName {
                name: " Subspace".into()
            }
        );
        assert_eq!(
            parse_display_name("a\nb").unwrap_err(),
            ProductError::UnsafeDisplayName {
                name: "a\nb".into()
            }
        );
    }

    #[test]
    fn parse_asset_path_rejects_escape_and_absolute_paths() {
        assert_eq!(
            parse_asset_path("assets/subspace.icns").unwrap(),
            "assets/subspace.icns"
        );
        for name in ["", "/tmp/x", "foo/../x", "foo//bar", r"foo\bar", "./x"] {
            assert_eq!(
                parse_asset_path(name).unwrap_err(),
                ProductError::UnsafeAssetPath {
                    name: name.to_string()
                }
            );
        }
    }

    #[test]
    fn parse_asset_path_rejects_windows_drive_nul_and_newline() {
        for name in ["C:/windows/x", "C:foo", "foo\0bar", "foo\nbar"] {
            assert_eq!(
                parse_asset_path(name).unwrap_err(),
                ProductError::UnsafeAssetPath {
                    name: name.to_string()
                }
            );
        }
        assert_eq!(
            parse_asset_path("assets/subspace.icns").unwrap(),
            "assets/subspace.icns"
        );
    }
}
