//! Injectable platform and environment inputs. Tests construct this type and
//! never read the process config or state directories.

use std::path::PathBuf;

use crate::error::ProductError;

/// Host family used for path selection. Independent of the compile target so
/// tests can cover Linux, macOS and Windows on any machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    Linux,
    Macos,
    Windows,
}

/// Directory and override inputs for path calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformEnv {
    pub os: OsKind,
    pub home: PathBuf,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
}

impl PlatformEnv {
    /// Reads the process environment. Tests construct [`PlatformEnv`] with
    /// synthetic roots instead of calling this.
    pub fn from_process() -> Result<Self, ProductError> {
        let home = dirs::home_dir().ok_or_else(|| ProductError::MissingHome {
            name: "HOME".into(),
        })?;
        let os = if cfg!(windows) {
            OsKind::Windows
        } else if cfg!(target_os = "macos") {
            OsKind::Macos
        } else {
            OsKind::Linux
        };
        Ok(Self {
            os,
            home,
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            app_data: std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            local_app_data: std::env::var_os("LOCALAPPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{OsKind, PlatformEnv};
    use std::path::PathBuf;

    #[test]
    fn platform_env_holds_synthetic_roots_without_reading_the_host() {
        let env = PlatformEnv {
            os: OsKind::Linux,
            home: PathBuf::from("/no-such-home/fixture"),
            xdg_config_home: Some(PathBuf::from("/no-such-xdg/config")),
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        };
        assert_eq!(env.os, OsKind::Linux);
        assert!(!env.home.exists());
        assert!(!env.xdg_config_home.as_ref().unwrap().exists());
    }
}
