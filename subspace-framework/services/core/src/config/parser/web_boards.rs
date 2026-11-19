use serde_json::{Map, Value};

use super::Parser;
use crate::Secret;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::web_boards::{
    MAX_WEB_BOARD_TITLE_CHARS, WebBoard, WebBoardSource, valid_web_board_id, web_board_url_allowed,
};

/// `webBoards`: board id to `{"title", "url"}`, in id order whatever the
/// JSON map keeps. An unusable entry is left out with a notice; the other
/// entries still load.
pub(super) fn parse(
    parser: &mut Parser<'_>,
    section: Option<&Map<String, Value>>,
) -> Vec<WebBoard> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut boards: Vec<WebBoard> = section
        .iter()
        .filter_map(|(id, entry)| match web_board(id, entry) {
            Ok(board) => Some(board),
            Err(message) => {
                parser.warn(format!("webBoards.{id}"), message);
                None
            }
        })
        .collect();
    boards.sort_by(|left, right| left.id.cmp(&right.id));
    boards
}

fn web_board(id: &str, entry: &Value) -> Result<WebBoard, String> {
    if !valid_web_board_id(id) {
        return Err(
            "expected an id of lower-case letters, digits and -, starting with a letter; ignored"
                .into(),
        );
    }
    let Value::Object(entry) = entry else {
        return Err("expected an object with a url or a path; ignored".into());
    };
    Ok(WebBoard {
        id: id.to_owned(),
        title: title(id, entry.get("title"))?,
        source: source(entry.get("url"), entry.get("path"))?,
        settings: values(entry.get("settings"))?,
        secrets: secrets(entry.get("secrets"))?,
        permissions: grants(entry.get("permissions"))?,
    })
}

fn grants(value: Option<&Value>) -> Result<BTreeMap<String, bool>, String> {
    object(value, "permissions")?
        .map(|(key, value)| {
            value
                .as_bool()
                .map(|on| (key.clone(), on))
                .ok_or_else(|| format!("expected permissions.{key} to be true or false; ignored"))
        })
        .collect()
}

/// `settings`: key to string, number or Boolean.
fn values(value: Option<&Value>) -> Result<BTreeMap<String, Value>, String> {
    object(value, "settings")?
        .map(|(key, value)| match value {
            Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                Ok((key.clone(), value.clone()))
            }
            _ => Err(format!(
                "expected settings.{key} to be a string, number or true or false; ignored"
            )),
        })
        .collect()
}

/// `secrets`: key to string.
fn secrets(value: Option<&Value>) -> Result<BTreeMap<String, Secret>, String> {
    object(value, "secrets")?
        .map(|(key, value)| match value {
            Value::String(secret) => Ok((key.clone(), Secret::new(secret.clone()))),
            _ => Err(format!("expected secrets.{key} to be a string; ignored")),
        })
        .collect()
}

/// The entries of an optional object; none when it is absent.
fn object<'v>(
    value: Option<&'v Value>,
    name: &str,
) -> Result<impl Iterator<Item = (&'v String, &'v Value)>, String> {
    match value {
        None | Some(Value::Null) => Ok(None.into_iter().flatten()),
        Some(Value::Object(entries)) => Ok(Some(entries.iter()).into_iter().flatten()),
        Some(_) => Err(format!("expected {name} to be an object; ignored")),
    }
}

fn source(url_value: Option<&Value>, path_value: Option<&Value>) -> Result<WebBoardSource, String> {
    match (url_value, path_value) {
        (Some(value), None) => url(Some(value)).map(WebBoardSource::Url),
        (None, Some(value)) => {
            folder(value, dirs::home_dir().as_deref()).map(WebBoardSource::Folder)
        }
        _ => Err("expected either a url or a path; ignored".into()),
    }
}

/// An absolute folder, or one under the home folder written as `~/…`.
fn folder(value: &Value, home: Option<&Path>) -> Result<PathBuf, String> {
    let text = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or("expected path to be a folder name; ignored")?;
    let path = match (text.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(text),
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("expected path to be an absolute folder, or one starting with ~/; ignored".into())
    }
}

fn title(id: &str, value: Option<&Value>) -> Result<String, String> {
    let title = match value {
        None | Some(Value::Null) => return Ok(id.to_owned()),
        Some(Value::String(title)) => title.trim(),
        Some(_) => return Err("expected title to be a string; ignored".into()),
    };
    if title.is_empty() || title.chars().count() > MAX_WEB_BOARD_TITLE_CHARS {
        return Err(format!(
            "expected a title of 1 to {MAX_WEB_BOARD_TITLE_CHARS} characters; ignored"
        ));
    }
    Ok(title.to_owned())
}

fn url(value: Option<&Value>) -> Result<String, String> {
    let url = value
        .and_then(Value::as_str)
        .and_then(|value| url::Url::parse(value.trim()).ok())
        .filter(web_board_url_allowed)
        .ok_or("expected url to be an https:// address, or http:// to localhost; ignored")?;
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::folder;
    use crate::config::parse;
    use crate::config::web_boards::{WebBoard, WebBoardSource};

    fn parsed(text: &str) -> (Vec<WebBoard>, Vec<String>) {
        let (config, notices) = parse(text, Path::new("/c/config.json"));
        (
            config.web_boards,
            notices.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn valid_entries_load_in_id_order_with_a_default_title() {
        let (boards, notices) = parsed(
            r#"{"webBoards": {
                "wiki": {"title": " Team wiki ", "url": "https://wiki.example.com/start"},
                "app": {"url": "http://localhost:5173"}
            }}"#,
        );
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(
            boards,
            [
                WebBoard {
                    id: "app".into(),
                    title: "app".into(),
                    source: WebBoardSource::Url("http://localhost:5173/".into()),
                    settings: Default::default(),
                    secrets: Default::default(),
                    permissions: Default::default(),
                },
                WebBoard {
                    id: "wiki".into(),
                    title: "Team wiki".into(),
                    source: WebBoardSource::Url("https://wiki.example.com/start".into()),
                    settings: Default::default(),
                    secrets: Default::default(),
                    permissions: Default::default(),
                },
            ]
        );
    }

    #[test]
    fn unusable_entries_are_left_out_with_a_notice() {
        let (boards, notices) = parsed(
            r#"{"webBoards": {
                "Bad": {"url": "https://example.com"},
                "plain": {"url": "http://example.com"},
                "secret": {"url": "https://user:pw@example.com"},
                "missing": {"title": "No address"},
                "text": "https://example.com",
                "long": {"title": "", "url": "https://example.com"},
                "both": {"url": "https://example.com", "path": "/srv/app"},
                "relative": {"path": "apps/notes"},
                "ok": {"url": "https://example.com"}
            }}"#,
        );
        assert_eq!(
            boards
                .iter()
                .map(|board| board.id.as_str())
                .collect::<Vec<_>>(),
            ["ok"]
        );
        assert_eq!(notices.len(), 8, "{notices:?}");
        assert!(
            notices
                .iter()
                .any(|notice| notice.contains("webBoards.secret"))
        );
        assert!(notices.iter().all(|notice| !notice.contains("pw")));
    }

    #[test]
    fn a_folder_entry_loads_an_absolute_path() {
        let root = std::env::temp_dir().join("notes-app");
        let text = json!({"webBoards": {"notes": {"title": "Notes", "path": root}}});
        let (boards, notices) = parsed(&text.to_string());
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(boards[0].source, WebBoardSource::Folder(root));
    }

    #[test]
    fn a_home_relative_folder_is_expanded() {
        let home = std::env::temp_dir();
        assert_eq!(
            folder(&json!("~/apps/notes"), Some(&home)),
            Ok(home.join("apps/notes"))
        );
        assert!(folder(&json!("~/apps/notes"), None).is_err());
        assert!(folder(&json!(""), Some(&home)).is_err());
        assert!(folder(&json!(3), Some(&home)).is_err());
    }

    #[test]
    fn settings_and_secrets_load_with_their_board() {
        let (boards, notices) = parsed(
            r#"{"webBoards": {"app": {
                "url": "https://app.example.com",
                "settings": {"endpoint": "https://api.example.com", "limit": 5, "compact": true},
                "secrets": {"token": "s3cret"}
            }}}"#,
        );
        assert!(notices.is_empty(), "{notices:?}");
        let board = &boards[0];
        assert_eq!(board.settings["limit"], json!(5));
        assert_eq!(board.settings["compact"], json!(true));
        assert_eq!(board.secrets["token"].expose(), "s3cret");
        assert_eq!(format!("{:?}", board.secrets), r#"{"token": ***}"#);
    }

    #[test]
    fn nested_settings_and_non_string_secrets_are_refused() {
        let (boards, notices) = parsed(
            r#"{"webBoards": {
                "nested": {"url": "https://a.example", "settings": {"x": {"y": 1}}},
                "number": {"url": "https://a.example", "secrets": {"token": 5}},
                "list": {"url": "https://a.example", "settings": ["x"]}
            }}"#,
        );
        assert!(boards.is_empty());
        assert_eq!(notices.len(), 3, "{notices:?}");
    }

    #[cfg(unix)]
    #[test]
    fn web_board_secrets_in_a_file_others_can_read_are_ignored() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let text = r#"{"webBoards": {"app": {"url": "https://a.example", "secrets": {"t": "x"}}}}"#;
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (config, notices) = crate::config::load(&file);
        assert!(config.web_boards[0].secrets.is_empty());
        assert!(notices[0].to_string().contains("chmod 600"));
    }

    #[test]
    fn a_web_boards_value_that_is_not_an_object_is_reported() {
        let (boards, notices) = parsed(r#"{"webBoards": ["https://example.com"]}"#);
        assert!(boards.is_empty());
        assert_eq!(notices.len(), 1);
    }
}
