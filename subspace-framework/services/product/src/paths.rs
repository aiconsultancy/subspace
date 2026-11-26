//! Product and board configuration path calculation. These functions do not
//! create directories.

use std::path::{Path, PathBuf};

use crate::channel::Channel;
use crate::data_name::data_name;
use crate::platform::{OsKind, PlatformEnv};

/// Cross-platform and native product configuration roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRoots {
    pub cross_platform: PathBuf,
    pub native: Option<PathBuf>,
}

impl ConfigRoots {
    /// Product roots in discovery order, skipping a native root that matches
    /// the cross-platform root.
    pub fn product_roots(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.cross_platform.clone()];
        if let Some(native) = &self.native
            && native != &self.cross_platform
        {
            paths.push(native.clone());
        }
        paths
    }
}

/// Product configuration roots for `binary` on `env`. Searching does not
/// create these directories.
pub fn product_config_roots(env: &PlatformEnv, binary: &str) -> ConfigRoots {
    match env.os {
        OsKind::Linux => ConfigRoots {
            cross_platform: linux_config_root(env, binary),
            native: None,
        },
        OsKind::Macos => ConfigRoots {
            cross_platform: env.home.join(".config").join(binary),
            native: Some(
                env.home
                    .join("Library")
                    .join("Application Support")
                    .join(binary),
            ),
        },
        OsKind::Windows => ConfigRoots {
            cross_platform: env.home.join(".config").join(binary),
            native: env.app_data.as_ref().map(|dir| dir.join(binary)),
        },
    }
}

fn linux_config_root(env: &PlatformEnv, binary: &str) -> PathBuf {
    match &env.xdg_config_home {
        Some(dir) => dir.join(binary),
        None => env.home.join(".config").join(binary),
    }
}

/// `<product-root>/<board-id>/` for a generic board directory.
// qual:api
pub fn board_root(product_root: &Path, board_id: &str) -> PathBuf {
    product_root.join(board_id)
}

/// The data directory name of this process's channel: the app's
/// [`data_name`] on stable, `<name>-<type>` for a pre-release type. Every
/// config, state, cache and log path uses it as its directory name in the
/// platform's config or state directory, and the Keychain service
/// `<data name>.github`, and any an app derives from it, are named after it.
pub fn product_data_name() -> String {
    Channel::current().name(data_name())
}

#[cfg(test)]
mod tests {
    use super::{ConfigRoots, board_root, product_config_roots, product_data_name};
    use crate::channel::Channel;
    use crate::platform::{OsKind, PlatformEnv};
    use std::path::{Path, PathBuf};

    fn env(os: OsKind, home: &str) -> PlatformEnv {
        PlatformEnv {
            os,
            home: PathBuf::from(home),
            xdg_config_home: None,
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        }
    }

    #[test]
    fn linux_uses_xdg_config_home_when_set() {
        let mut linux = env(OsKind::Linux, "/no-such-home/fixture");
        linux.xdg_config_home = Some(PathBuf::from("/custom/config"));
        let roots = product_config_roots(&linux, "subspace");
        assert_eq!(
            roots.cross_platform,
            PathBuf::from("/custom/config/subspace")
        );
        assert_eq!(roots.native, None);
        assert!(!roots.cross_platform.exists());
    }

    #[test]
    fn linux_falls_back_to_home_config() {
        let roots = product_config_roots(&env(OsKind::Linux, "/no-such-home/fixture"), "subspace");
        assert_eq!(
            roots.cross_platform,
            PathBuf::from("/no-such-home/fixture/.config/subspace")
        );
        assert_eq!(roots.product_roots(), vec![roots.cross_platform.clone()]);
    }

    #[test]
    fn macos_keeps_literal_xdg_and_native_roots() {
        let roots = product_config_roots(&env(OsKind::Macos, "/no-such-home/fixture"), "subspace");
        assert_eq!(
            roots.product_roots(),
            vec![
                PathBuf::from("/no-such-home/fixture/.config/subspace"),
                PathBuf::from("/no-such-home/fixture/Library/Application Support/subspace"),
            ]
        );
        assert!(!roots.cross_platform.exists());
        assert!(!roots.native.as_ref().unwrap().exists());
    }

    #[test]
    fn windows_uses_home_config_and_appdata() {
        let home = PathBuf::from(r"C:\Users\fixture");
        let app = PathBuf::from(r"C:\Users\fixture\AppData\Roaming");
        let mut windows = env(OsKind::Windows, r"C:\Users\fixture");
        windows.app_data = Some(app.clone());
        let roots = product_config_roots(&windows, "subspace");
        assert_eq!(
            roots.product_roots(),
            vec![home.join(".config").join("subspace"), app.join("subspace")]
        );
    }

    #[test]
    fn board_root_is_a_generic_child_of_the_product_root() {
        let root = PathBuf::from("/no-such-home/fixture/.config/subspace");
        assert_eq!(
            board_root(&root, "github"),
            PathBuf::from("/no-such-home/fixture/.config/subspace/github")
        );
    }

    #[test]
    fn each_type_has_its_own_config_directory() {
        let mac = env(OsKind::Macos, "/no-such-home/fixture");
        let mut seen = Vec::new();
        for channel in Channel::ALL {
            let roots = product_config_roots(&mac, &channel.name("subspace"));
            assert!(!seen.contains(&roots.native), "{channel:?}");
            seen.push(roots.native);
        }
        let nightly = product_config_roots(&mac, &Channel::Nightly.name("subspace"));
        assert_eq!(
            nightly.native.as_deref(),
            Some(Path::new(
                "/no-such-home/fixture/Library/Application Support/subspace-nightly"
            ))
        );
        assert_eq!(product_data_name(), Channel::current().name("subspace"));
    }

    #[test]
    fn product_roots_skip_native_when_it_matches_cross_platform() {
        let roots = ConfigRoots {
            cross_platform: PathBuf::from("/same"),
            native: Some(PathBuf::from("/same")),
        };
        assert_eq!(roots.product_roots(), vec![PathBuf::from("/same")]);
    }
}
