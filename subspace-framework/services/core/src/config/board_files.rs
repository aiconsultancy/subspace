//! Native boards' own settings files beside the config file:
//! `<config directory>/<board id>/config.json`. The top-level object of
//! each file is that board's settings. Like the config file, a board file
//! may hold passwords, so one that other users can read or write is ignored.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde_json::{Map, Value};

use super::{Notice, open_to_others, warning};

/// The settings file of board `id` beside the config file `config_file`.
pub fn board_file(config_file: &Path, id: &str) -> PathBuf {
    config_file
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(id)
        .join("config.json")
}

/// The settings of each board in `ids` that has a file, with a notice for
/// each file that cannot be used. A missing file or folder is no notice.
pub(super) fn load<'a>(
    config_file: &Path,
    ids: impl IntoIterator<Item = &'a str>,
) -> (BTreeMap<String, Map<String, Value>>, Vec<Notice>) {
    let mut settings = BTreeMap::new();
    let mut notices = Vec::new();
    for id in ids {
        match read(&board_file(config_file, id)) {
            Ok(Some(board)) => {
                settings.insert(id.to_owned(), board);
            }
            Ok(None) => {}
            Err(notice) => notices.push(notice),
        }
    }
    (settings, notices)
}

fn read(file: &Path) -> Result<Option<Map<String, Value>>, Notice> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(warning(file, format!("cannot read the file: {error}"))),
    };
    let settings = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(settings)) => settings,
        Ok(_) => return Err(warning(file, "expected a JSON object; using the defaults")),
        Err(error) => {
            return Err(warning(
                file,
                format!("invalid JSON ({error}); using the defaults"),
            ));
        }
    };
    if open_to_others(file) {
        return Err(warning(
            file,
            format!(
                "other users can read or write this file, so Subspace ignores it; \
                 run `chmod 600 {}`",
                file.display()
            ),
        ));
    }
    Ok(Some(settings))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;

    use super::{board_file, load};

    fn private_write(file: &Path, text: &str) {
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
        crate::private_file::set_owner_only(file).unwrap();
    }

    #[test]
    fn a_board_file_sits_in_the_boards_folder_beside_the_config_file() {
        assert_eq!(
            board_file(
                Path::new("/home/fixture/.config/subspace/config.json"),
                "irc"
            ),
            Path::new("/home/fixture/.config/subspace/irc/config.json")
        );
    }

    #[test]
    fn existing_board_files_load_whole_and_missing_ones_are_silent() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.json");
        private_write(
            &board_file(&config, "irc"),
            r#"{"networks": [{"id": "local"}], "future": {"keep": 7}}"#,
        );
        let (settings, notices) = load(&config, ["irc", "github"]);
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(settings.len(), 1);
        assert_eq!(settings["irc"]["networks"][0]["id"], "local");
        assert_eq!(settings["irc"]["future"], json!({"keep": 7}));
    }

    #[test]
    fn unusable_board_files_are_named_in_notices_and_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.json");
        private_write(&board_file(&config, "irc"), "{ nope");
        private_write(&board_file(&config, "list"), "[]");
        let (settings, notices) = load(&config, ["irc", "list"]);
        assert!(settings.is_empty());
        let notices: Vec<String> = notices.iter().map(ToString::to_string).collect();
        let irc = board_file(&config, "irc").display().to_string();
        let list = board_file(&config, "list").display().to_string();
        assert!(
            notices[0].starts_with(&format!("{irc}: invalid JSON")),
            "{notices:?}"
        );
        assert_eq!(
            notices[1],
            format!("{list}: expected a JSON object; using the defaults")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_board_file_others_can_read_is_ignored() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.json");
        let file = board_file(&config, "irc");
        private_write(&file, r#"{"networks": []}"#);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        let (settings, notices) = load(&config, ["irc"]);
        assert!(settings.is_empty());
        assert_eq!(notices.len(), 1);
        assert!(notices[0].to_string().contains("chmod 600"));
        assert_eq!(notices[0].file, file);
    }
}
