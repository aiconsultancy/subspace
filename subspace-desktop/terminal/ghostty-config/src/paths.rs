//! Injectable filesystem paths for configuration and theme search, so tests
//! never read the real home directory.

use std::path::PathBuf;

/// The directories Ghostty and the Subspace layer read configuration from.
///
/// Every field is a directory. `xdg_config_home` is the XDG config directory
/// (defaulting to `~/.config`), `macos_app_support` is
/// `~/Library/Application Support/com.mitchellh.ghostty` on macOS (`None`
/// elsewhere), and `subspace_layer` is `~/.config/subspace/ghostty`.
#[derive(Debug, Clone)]
pub struct Paths {
    pub xdg_config_home: PathBuf,
    pub macos_app_support: Option<PathBuf>,
    pub subspace_layer: PathBuf,
}

impl Paths {
    /// Returns the configuration files in REQ-29 load order: later files
    /// override earlier ones. Missing files are filtered by the caller, not
    /// here.
    pub fn ordered_config_files(&self) -> Vec<PathBuf> {
        let mut files = vec![
            self.xdg_config_home.join("ghostty/config"),
            self.xdg_config_home.join("ghostty/config.ghostty"),
        ];
        if let Some(app_support) = &self.macos_app_support {
            files.push(app_support.join("config"));
            files.push(app_support.join("config.ghostty"));
        }
        files.push(self.subspace_layer.join("config"));
        files.push(self.subspace_layer.join("config.ghostty"));
        files
    }
}

#[cfg(test)]
mod tests {
    use super::Paths;
    use std::path::PathBuf;

    fn sample_paths() -> Paths {
        Paths {
            xdg_config_home: PathBuf::from("/home/fixture/.config"),
            macos_app_support: Some(PathBuf::from(
                "/home/fixture/Library/Application Support/com.mitchellh.ghostty",
            )),
            subspace_layer: PathBuf::from("/home/fixture/.config/subspace/ghostty"),
        }
    }

    #[test]
    fn ordered_config_files_lists_xdg_then_macos_then_subspace_layer() {
        let paths = sample_paths();
        let files = paths.ordered_config_files();
        assert_eq!(
            files,
            vec![
                PathBuf::from("/home/fixture/.config/ghostty/config"),
                PathBuf::from("/home/fixture/.config/ghostty/config.ghostty"),
                PathBuf::from(
                    "/home/fixture/Library/Application Support/com.mitchellh.ghostty/config"
                ),
                PathBuf::from(
                    "/home/fixture/Library/Application Support/com.mitchellh.ghostty/config.ghostty"
                ),
                PathBuf::from("/home/fixture/.config/subspace/ghostty/config"),
                PathBuf::from("/home/fixture/.config/subspace/ghostty/config.ghostty"),
            ]
        );
    }

    #[test]
    fn ordered_config_files_omits_macos_pair_when_absent() {
        let mut paths = sample_paths();
        paths.macos_app_support = None;
        let files = paths.ordered_config_files();
        assert_eq!(files.len(), 4);
    }
}
