//! The app's data directory name: one per process, so every config, state,
//! cache and log path uses the same directory name, as do the Keychain
//! service `<data name>.github` and any service an app derives from it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::ProductError;
use crate::identity::parse_identity;

/// The name every data path uses, fixed by the first
/// [`configure_data_name`] or, failing that, by the first [`data_name`].
static DATA_NAME: OnceLock<DataName> = OnceLock::new();

/// The data directory name of a process that reads a path before naming one.
pub const DEFAULT_DATA_NAME: &str = "subspace";

#[derive(Debug)]
struct DataName {
    name: String,
    /// False while only a read has fixed [`DEFAULT_DATA_NAME`]; naming that
    /// same name sets it.
    configured: AtomicBool,
}

/// Why [`configure_data_name`] did not set the name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DataNameError {
    /// The name is not an identity: ASCII letters, digits, `-` and `_`,
    /// starting with a letter or digit.
    #[error("the data directory name is not valid: {0}")]
    Invalid(ProductError),
    /// An earlier call chose another name, which stays.
    #[error("the data directory is already named '{0}'")]
    Configured(String),
    /// A data path was read before the app named its data directory, so
    /// the process already uses the default name.
    #[error(
        "a data path was read under '{0}' before the app named its data directory; call configure_data_name before anything reads a path"
    )]
    Resolved(String),
}

/// Names the stable data directory, such as `example-app`; a pre-release
/// channel adds its type. Call it before anything reads a data path. The
/// first name stays: naming the name already in use succeeds, even after a
/// read fixed [`DEFAULT_DATA_NAME`], and counts as the app's choice from
/// then on; naming another is an error that changes nothing.
pub fn configure_data_name(name: &str) -> Result<(), DataNameError> {
    let name = parse_identity(name).map_err(DataNameError::Invalid)?;
    settle(fix(name.as_str(), true), name.as_str())
}

/// The stable data directory name. The first read without a configured
/// name fixes [`DEFAULT_DATA_NAME`] for the rest of the process.
pub fn data_name() -> &'static str {
    &fix(DEFAULT_DATA_NAME, false).name
}

fn fix(name: &str, configured: bool) -> &'static DataName {
    DATA_NAME.get_or_init(|| DataName {
        name: name.to_owned(),
        configured: AtomicBool::new(configured),
    })
}

fn settle(current: &DataName, requested: &str) -> Result<(), DataNameError> {
    if current.name == requested {
        current.configured.store(true, Ordering::Relaxed);
        Ok(())
    } else if current.configured.load(Ordering::Relaxed) {
        Err(DataNameError::Configured(current.name.clone()))
    } else {
        Err(DataNameError::Resolved(current.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::{DataName, DataNameError, configure_data_name, settle};
    use crate::error::ProductError;

    #[test]
    fn names_that_are_not_identities_are_refused_before_anything_is_set() {
        for name in ["", "..", "a/b", r"a\b", "/etc", "-x", "app name"] {
            assert_eq!(
                configure_data_name(name),
                Err(DataNameError::Invalid(ProductError::UnsafeIdentity {
                    name: name.to_owned()
                })),
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_first_name_stays() {
        let named = |name: &str, configured| DataName {
            name: name.to_owned(),
            configured: AtomicBool::new(configured),
        };
        assert_eq!(settle(&named("example-app", true), "example-app"), Ok(()));
        assert_eq!(
            settle(&named("example-app", true), "harbor"),
            Err(DataNameError::Configured("example-app".into()))
        );
        let read = named("subspace", false);
        assert_eq!(
            settle(&read, "harbor"),
            Err(DataNameError::Resolved("subspace".into()))
        );
        assert_eq!(settle(&read, "subspace"), Ok(()));
        assert_eq!(
            settle(&read, "harbor"),
            Err(DataNameError::Configured("subspace".into())),
            "naming the name in use after a read is the app's choice"
        );
    }
}
