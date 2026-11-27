use crate::{
    Secret,
    config::{self, ChatEngine, Config, NoticeKind},
    state,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;

mod board_fields;
mod board_file;
mod merge;
#[cfg(test)]
mod native_board_tests;
mod repos;
mod web_boards;

pub use board_fields::BoardFieldEdits;

#[derive(Clone)]
pub enum TokenEdit {
    Keep,
    Replace(Secret),
    Clear,
}
#[derive(Clone)]
pub struct ServiceSettings {
    pub mcp: config::McpSettings,
    pub chat_engine: ChatEngine,
    /// `chat.acp.agent`. Its command overrides stay as the file has them.
    pub acp_agent: config::AcpAgent,
    pub board_order: Vec<String>,
    pub auto_refresh: bool,
    /// The config file's `terminal` object. Save sets each key, and removes
    /// one whose value is `null`.
    pub terminal: Map<String, Value>,
    /// Explicit board choices from the config file. A missing id is enabled.
    pub boards: BTreeMap<String, bool>,
    /// Explicit registered-feature choices, including valid future flags.
    pub features: BTreeMap<String, bool>,
    /// The favourite repositories, `owner/name`, in saved order.
    pub repos: Vec<String>,
    pub github_token: TokenEdit,
    /// Changes to boards' own run-time settings; empty when none changed.
    pub board_fields: Vec<BoardFieldEdits>,
    /// Changes to native boards' own settings files, keyed by board id:
    /// the top-level keys to set. Empty when none changed.
    pub board_files: BTreeMap<String, Map<String, Value>>,
}
#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("cannot read settings file: {0}")]
    Io(#[from] io::Error),
    #[error("settings file contains invalid JSON")]
    Json(#[from] serde_json::Error),
    #[error("settings file has invalid values")]
    Invalid,
    #[error("invalid {field}: {reason}")]
    Validation {
        field: &'static str,
        reason: &'static str,
    },
    #[error("settings file changed externally; reopen it before saving")]
    Stale,
    #[error("{}: {error}", file.display())]
    BoardFile {
        file: PathBuf,
        #[source]
        error: Box<SettingsError>,
    },
}
pub struct SettingsFile {
    file: PathBuf,
    original: Option<String>,
    root: Map<String, Value>,
    values: ServiceSettings,
    github_token_present: bool,
    /// The native boards' own files opened so far, keyed by board id.
    boards: BTreeMap<String, board_file::BoardFile>,
    /// The former Space ids the file was read with.
    renames: crate::former_ids::Renames,
}

impl SettingsFile {
    /// Opens the config file for editing. Settings saved under a Space's
    /// former id are read, and later saved, as the Space's own (see
    /// [`crate::former_ids`]).
    pub fn open(file: &Path) -> Result<Self, SettingsError> {
        Self::open_with(file, &crate::former_ids::installed())
    }

    /// As [`SettingsFile::open`], with the former ids `renames`.
    pub(crate) fn open_with(
        file: &Path,
        renames: &crate::former_ids::Renames,
    ) -> Result<Self, SettingsError> {
        let original = read_existing(file)?;
        let mut root = match original.as_deref() {
            None => Map::new(),
            Some(text) => match serde_json::from_str::<Value>(text)? {
                Value::Object(v) => v,
                _ => return Err(SettingsError::Invalid),
            },
        };
        crate::former_ids::rename_config(&mut root, renames);
        let (config, notices) = match original.as_deref() {
            None => (Config::default(), Vec::new()),
            Some(text) => config::parse_with(text, file, renames),
        };
        if notices
            .iter()
            .any(|notice| notice.kind == NoticeKind::Warning)
        {
            return Err(SettingsError::Invalid);
        }
        let github_token_present = config.github.token.is_some();
        let values = from_config(config, &root);
        Ok(Self {
            file: file.into(),
            original,
            root,
            values,
            github_token_present,
            boards: BTreeMap::new(),
            renames: renames.clone(),
        })
    }
    pub fn values(&self) -> &ServiceSettings {
        &self.values
    }
    /// Opens native board `id`'s own settings file for this transaction, once,
    /// and returns its saved settings. A missing file has none.
    pub fn open_board_file(&mut self, id: &str) -> Result<Map<String, Value>, SettingsError> {
        if !self.boards.contains_key(id) {
            let file = self.board_path(id);
            let opened = board_file::BoardFile::open(file.clone())
                .map_err(|error| board_error(&file, error))?;
            self.boards.insert(id.to_owned(), opened);
        }
        Ok(self.board_file_values(id))
    }
    /// Native board `id`'s settings file as it resolves now (see
    /// [`config::board_file`]).
    fn board_path(&self, id: &str) -> PathBuf {
        let dir = self.file.parent().unwrap_or_else(|| Path::new(""));
        crate::former_ids::board_file(dir, id, &self.renames)
    }
    /// The saved settings of native board `id`'s opened file.
    pub fn board_file_values(&self, id: &str) -> Map<String, Value> {
        self.boards
            .get(id)
            .map(|board| board.values().clone())
            .unwrap_or_default()
    }
    /// The saved values of the board whose config section is `section`.
    pub fn board_values(&self, section: &[String]) -> Map<String, Value> {
        board_fields::values(&self.root, section)
    }
    /// Whether the board at `section` has a saved secret `key`.
    pub fn board_secret_present(&self, section: &[String], key: &str) -> bool {
        board_fields::secret_present(&self.root, section, key)
    }
    /// The keys of the secrets saved for the board at `section`, never
    /// their values.
    pub fn board_secret_keys(&self, section: &[String]) -> Vec<String> {
        board_fields::secret_keys(&self.root, section)
    }
    pub fn board_permission(&self, section: &[String], key: &str) -> bool {
        board_fields::permission(&self.root, section, key)
    }
    pub fn token_present(&self, section: &str) -> bool {
        section == "github" && self.github_token_present
    }
    /// Writes the config file, then each changed board file. Every file is
    /// checked before the first write, so a conflict writes nothing.
    pub fn save(&mut self, values: &ServiceSettings) -> Result<(), SettingsError> {
        self.check_current()?;
        let mut root = self.root.clone();
        merge(&mut root, values, self.github_token_present)?;
        let boards = self.edited_board_files(&values.board_files)?;
        self.write_root(root)?;
        self.write_board_files(boards)
    }

    /// Persist the switcher order without changing other settings.
    pub fn save_board_order(&mut self, order: &[String]) -> Result<(), SettingsError> {
        let mut root = self.root.clone();
        merge::write_board_order(&mut root, order);
        self.write_root(root)
    }

    /// Persist the board choices (`boards`) without changing other settings,
    /// as Save writes them: a missing id uses the product default.
    pub fn save_board_choices(
        &mut self,
        boards: &BTreeMap<String, bool>,
    ) -> Result<(), SettingsError> {
        let mut root = self.root.clone();
        merge::write_board_choices(&mut root, boards);
        self.write_root(root)
    }

    fn check_current(&self) -> Result<(), SettingsError> {
        if read_existing(&self.file)? != self.original {
            return Err(SettingsError::Stale);
        }
        Ok(())
    }

    fn edited_board_files(
        &mut self,
        edits: &BTreeMap<String, Map<String, Value>>,
    ) -> Result<Vec<(String, Map<String, Value>)>, SettingsError> {
        let mut edited = Vec::with_capacity(edits.len());
        for (id, changes) in edits {
            self.open_board_file(id)?;
            let Some(board) = self.boards.get(id) else {
                continue;
            };
            let root = board
                .edited_at(&self.board_path(id), changes)
                .map_err(|error| board_error(board.path(), error))?;
            edited.push((id.clone(), root));
        }
        Ok(edited)
    }

    fn write_board_files(
        &mut self,
        edited: Vec<(String, Map<String, Value>)>,
    ) -> Result<(), SettingsError> {
        for (id, root) in edited {
            if let Some(board) = self.boards.get_mut(&id) {
                let file = board.path().to_owned();
                board
                    .write(root)
                    .map_err(|error| board_error(&file, error))?;
            }
        }
        Ok(())
    }

    fn write_root(&mut self, root: Map<String, Value>) -> Result<(), SettingsError> {
        let text = serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n";
        let (config, notices) = config::parse(&text, &self.file);
        if notices
            .iter()
            .any(|notice| notice.kind == NoticeKind::Warning)
        {
            return Err(SettingsError::Validation {
                field: "settings",
                reason: "values do not satisfy config constraints",
            });
        }
        self.check_current()?;
        state::write_atomic(&self.file, &text)?;
        self.original = Some(text);
        self.root = root;
        self.github_token_present = config.github.token.is_some();
        self.values = from_config(config, &self.root);
        Ok(())
    }
}
fn from_config(c: Config, root: &Map<String, Value>) -> ServiceSettings {
    let repos = root
        .get("repos")
        .and_then(Value::as_array)
        .filter(|items| items.iter().all(Value::is_string))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    ServiceSettings {
        mcp: c.mcp,
        chat_engine: c.chat.engine,
        acp_agent: c.chat.acp.agent,
        board_order: c.board_order,
        auto_refresh: c.auto_refresh,
        terminal: c.terminal,
        boards: c.boards,
        features: c.features,
        repos,
        github_token: TokenEdit::Keep,
        board_fields: Vec::new(),
        board_files: BTreeMap::new(),
    }
}
/// The file's text, or `None` when it does not exist.
fn read_existing(file: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn board_error(file: &Path, error: SettingsError) -> SettingsError {
    SettingsError::BoardFile {
        file: file.to_owned(),
        error: Box::new(error),
    }
}
fn section(root: &Map<String, Value>, key: &str) -> Map<String, Value> {
    root.get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}
fn token(section: &mut Map<String, Value>, edit: &TokenEdit, present: bool) {
    match edit {
        TokenEdit::Keep => {
            if !present {
                section.remove("token");
            }
        }
        TokenEdit::Replace(v) => {
            section.insert("token".into(), Value::String(v.expose().into()));
        }
        TokenEdit::Clear => {
            section.remove("token");
        }
    }
}
fn merge(
    root: &mut Map<String, Value>,
    v: &ServiceSettings,
    github_token_present: bool,
) -> Result<(), SettingsError> {
    merge::validate(v)?;
    board_fields::validate(&v.board_fields)?;
    merge::write_values(root, v, github_token_present)?;
    board_fields::write(root, &v.board_fields)
}

#[cfg(test)]
mod tests {
    use super::{ChatEngine, ServiceSettings, SettingsError, SettingsFile, TokenEdit};
    use crate::Secret;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::fs;

    fn values() -> ServiceSettings {
        ServiceSettings {
            mcp: crate::config::McpSettings::default(),
            chat_engine: ChatEngine::Copilot,
            acp_agent: crate::config::AcpAgent::OpenCode,
            board_order: Vec::new(),
            auto_refresh: true,
            terminal: serde_json::Map::new(),
            boards: BTreeMap::new(),
            features: BTreeMap::new(),
            repos: vec!["owner/repo".into()],
            github_token: TokenEdit::Keep,
            board_fields: Vec::new(),
            board_files: BTreeMap::new(),
        }
    }

    #[test]
    fn board_fields_are_saved_in_the_same_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            r#"{"webBoards": {"notes": {"url": "https://notes.example"}}}"#,
        )
        .unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        let section = vec!["webBoards".to_owned(), "notes".to_owned()];
        let mut settings = values();
        settings.board_fields = vec![super::BoardFieldEdits {
            section: section.clone(),
            values: BTreeMap::from([("limit".into(), Some(Value::from(5)))]),
            secrets: BTreeMap::from([("token".into(), TokenEdit::Replace(Secret::new("t")))]),
            permissions: BTreeMap::new(),
        }];
        file.save(&settings).unwrap();
        assert_eq!(file.board_values(&section)["limit"], Value::from(5));
        assert!(file.board_secret_present(&section, "token"));
        let (config, notices) = crate::config::load(&path);
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(config.web_boards[0].secrets["token"].expose(), "t");
        assert!(file.values().board_fields.is_empty());
    }

    #[test]
    fn mcp_round_trips_every_key_and_preserves_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"mcp":{"extension":7},"unknown":true}"#).unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        let mut settings = file.values().clone();
        settings.mcp.enabled = true;
        settings.mcp.bind = "::1".parse().unwrap();
        settings.mcp.port = 7400;
        settings.mcp.require_token = true;
        settings.mcp.groups.insert("github".into(), false);
        file.save(&settings).unwrap();
        let reopened = SettingsFile::open(&path).unwrap();
        assert_eq!(reopened.values().mcp, settings.mcp);
        assert_eq!(json(&path)["mcp"]["extension"], 7);
        assert_eq!(json(&path)["unknown"], true);
        let before = fs::read(&path).unwrap();
        settings.mcp.bind = "0.0.0.0".parse().unwrap();
        settings.mcp.require_token = false;
        assert!(file.save(&settings).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn board_order_refresh_and_terminal_settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, "{}\n").unwrap();
        let mut settings = SettingsFile::open(&file).unwrap();
        let mut values = settings.values().clone();
        values.board_order = vec!["irc".into(), "github".into()];
        values.auto_refresh = false;
        values.terminal = serde_json::json!({"fontFamily": "Menlo", "fontSize": null})
            .as_object()
            .cloned()
            .unwrap();
        settings.save(&values).unwrap();

        let reopened = SettingsFile::open(&file).unwrap();
        assert_eq!(reopened.values().board_order, ["irc", "github"]);
        assert!(!reopened.values().auto_refresh);
        assert_eq!(
            Value::Object(reopened.values().terminal.clone()),
            serde_json::json!({"fontFamily": "Menlo"})
        );
    }

    #[test]
    fn board_choices_save_alone_and_match_a_full_save() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, r#"{"boards": {"irc": true}, "autoRefresh": false}"#).unwrap();
        let mut alone = SettingsFile::open(&file).unwrap();
        let mut boards = alone.values().boards.clone();
        boards.insert("github".into(), false);
        alone.save_board_choices(&boards).unwrap();
        assert_eq!(
            json(&file),
            serde_json::json!({"boards": {"github": false, "irc": true}, "autoRefresh": false})
        );

        let full = dir.path().join("full.json");
        fs::write(&full, r#"{"boards": {"irc": true}, "autoRefresh": false}"#).unwrap();
        let mut saved = SettingsFile::open(&full).unwrap();
        let mut values = saved.values().clone();
        values.boards.clone_from(&boards);
        saved.save(&values).unwrap();
        assert_eq!(json(&full)["boards"], json(&file)["boards"]);
        assert_eq!(alone.values().boards, saved.values().boards);

        alone.save_board_choices(&BTreeMap::new()).unwrap();
        assert!(json(&file).get("boards").is_none());
    }

    fn source() -> &'static str {
        r#"{
          "unknown": {"keep": true, "nested": {"value": 7}},
          "github": {"token":"gh", "other": {"keep": "yes"}},
          "terminal": {"fontSize": 13, "extension": {"keep": 7}},
          "repos": ["owner/repo"]
        }"#
    }
    fn json(file: &std::path::Path) -> Value {
        serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
    }

    #[test]
    fn preserves_unknown_properties_and_all_kept_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source()).unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        let kept = editor.values().clone();
        editor.save(&kept).unwrap();
        let root = json(&file);
        assert_eq!(
            root["unknown"],
            serde_json::json!({"keep":true,"nested":{"value":7}})
        );
        assert_eq!(root["github"]["other"], serde_json::json!({"keep":"yes"}));
        assert_eq!(root["github"]["token"], "gh");
        assert_eq!(root["terminal"]["fontSize"], 13);
        assert_eq!(root["terminal"]["extension"], serde_json::json!({"keep":7}));
        assert_eq!(root["repos"], serde_json::json!(["owner/repo"]));
        assert!(matches!(editor.values().github_token, TokenEdit::Keep));
    }

    #[test]
    fn chat_engine_saves_without_replacing_unknown_chat_settings() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(
            &file,
            r#"{"chat":{"engine":"copilot","extension":{"keep":true}},"other":7}"#,
        )
        .unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        let mut edited = editor.values().clone();
        edited.chat_engine = crate::config::ChatEngine::Pi;
        editor.save(&edited).unwrap();

        let saved = json(&file);
        assert_eq!(saved["chat"]["engine"], "pi");
        assert_eq!(saved["chat"]["extension"], serde_json::json!({"keep":true}));
        assert_eq!(saved["other"], 7);
        assert_eq!(
            crate::config::load(&file).0.chat.engine,
            crate::config::ChatEngine::Pi
        );
    }

    #[test]
    fn acp_agent_saves_without_replacing_its_command_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(
            &file,
            r#"{"chat":{"engine":"copilot","acp":{"agent":"opencode","command":"/opt/agent","args":["acp"]}}}"#,
        )
        .unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        let mut edited = editor.values().clone();
        edited.chat_engine = ChatEngine::Acp;
        edited.acp_agent = crate::config::AcpAgent::Claude;
        editor.save(&edited).unwrap();

        let saved = json(&file);
        assert_eq!(saved["chat"]["engine"], "acp");
        assert_eq!(saved["chat"]["acp"]["agent"], "claude");
        assert_eq!(saved["chat"]["acp"]["command"], "/opt/agent");
        assert_eq!(saved["chat"]["acp"]["args"], serde_json::json!(["acp"]));
        let config = crate::config::load(&file).0;
        assert_eq!(config.chat.acp.agent, crate::config::AcpAgent::Claude);
    }

    #[test]
    fn cancelling_or_conflicting_chat_engine_edits_does_not_write_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let original = b"{\"chat\":{\"engine\":\"copilot\"}}\n";
        fs::write(&file, original).unwrap();
        let editor = SettingsFile::open(&file).unwrap();
        let mut cancelled = editor.values().clone();
        cancelled.chat_engine = crate::config::ChatEngine::Pi;
        drop(editor);
        assert_eq!(fs::read(&file).unwrap(), original);

        let mut editor = SettingsFile::open(&file).unwrap();
        let mut edited = editor.values().clone();
        edited.chat_engine = crate::config::ChatEngine::Pi;
        fs::write(&file, r#"{"external":true}"#).unwrap();
        assert!(matches!(editor.save(&edited), Err(SettingsError::Stale)));
        assert_eq!(json(&file), serde_json::json!({"external":true}));
    }

    #[test]
    fn board_choices_save_transactionally_and_omitted_boards_stay_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(
            &file,
            r#"{"boards":{"irc":false,"excluded":true},"extension":{"keep":true}}"#,
        )
        .unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        assert!(!editor.values().boards["irc"]);
        assert!(!editor.values().boards.contains_key("github"));

        let mut edited = editor.values().clone();
        edited.boards.remove("irc");
        edited.boards.insert("github".into(), false);
        editor.save(&edited).unwrap();

        let saved = json(&file);
        assert_eq!(
            saved["boards"],
            serde_json::json!({"excluded": true, "github": false})
        );
        assert_eq!(saved["extension"], serde_json::json!({"keep": true}));
        let config = crate::config::load(&file).0;
        assert!(config.board_enabled("irc"));
        assert!(!config.board_enabled("github"));
        assert!(config.board_enabled("actions"));
    }

    #[test]
    fn opening_and_cancelling_board_edits_does_not_write_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let original = b"{\n  \"boards\": {\"github-copilot-agents\": false}\n}\n";
        fs::write(&file, original).unwrap();
        let editor = SettingsFile::open(&file).unwrap();
        let mut cancelled = editor.values().clone();
        cancelled.boards.remove("github-copilot-agents");
        drop(editor);
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[test]
    fn feature_choices_preserve_unknown_flags_and_save_transactionally() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(
            &file,
            r#"{"features":{"app.pi-chat":false,"board.future.preview":true},"extension":{"keep":7}}"#,
        )
        .unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        assert!(editor.values().features["board.future.preview"]);
        let mut edited = editor.values().clone();
        edited.features.insert("app.pi-chat".into(), true);
        editor.save(&edited).unwrap();

        let saved = json(&file);
        assert_eq!(saved["features"]["app.pi-chat"], true);
        assert_eq!(saved["features"]["board.future.preview"], true);
        assert_eq!(saved["extension"]["keep"], 7);
        assert!(crate::config::load(&file).0.features["board.future.preview"]);
    }

    #[test]
    fn cancelling_or_conflicting_feature_edits_does_not_write_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let original = b"{\"features\":{\"app.pi-chat\":false}}\n";
        fs::write(&file, original).unwrap();
        let editor = SettingsFile::open(&file).unwrap();
        let mut cancelled = editor.values().clone();
        cancelled.features.insert("app.pi-chat".into(), true);
        drop(editor);
        assert_eq!(fs::read(&file).unwrap(), original);

        let mut editor = SettingsFile::open(&file).unwrap();
        let mut edited = editor.values().clone();
        edited.features.insert("app.pi-chat".into(), true);
        fs::write(&file, r#"{"external":true}"#).unwrap();
        assert!(matches!(editor.save(&edited), Err(SettingsError::Stale)));
        assert_eq!(json(&file), serde_json::json!({"external":true}));
    }

    #[test]
    fn malformed_feature_keys_or_values_refuse_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        for source in [
            r#"{"features":{"not-namespaced":true}}"#,
            r#"{"features":{"app.pi-chat":"true"}}"#,
        ] {
            let file = dir.path().join(format!("{}.json", source.len()));
            fs::write(&file, source).unwrap();
            assert!(matches!(
                SettingsFile::open(&file),
                Err(SettingsError::Invalid)
            ));
        }
    }

    #[test]
    fn replaces_and_clears_tokens_without_retaining_secret() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source()).unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        let mut edited = editor.values().clone();
        edited.github_token = TokenEdit::Replace(Secret::new("replacement"));
        editor.save(&edited).unwrap();
        assert_eq!(json(&file)["github"]["token"], "replacement");
        assert!(matches!(editor.values().github_token, TokenEdit::Keep));
        edited = editor.values().clone();
        edited.github_token = TokenEdit::Clear;
        editor.save(&edited).unwrap();
        assert!(json(&file)["github"].get("token").is_none());
        assert!(matches!(editor.values().github_token, TokenEdit::Keep));
    }

    #[test]
    fn distinguishes_missing_empty_and_externally_created_files() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        let mut editor = SettingsFile::open(&missing).unwrap();
        fs::write(&missing, "").unwrap();
        assert!(matches!(editor.save(&values()), Err(SettingsError::Stale)));
        assert!(matches!(
            SettingsFile::open(&missing),
            Err(SettingsError::Json(_))
        ));
        let empty = dir.path().join("empty.json");
        fs::write(&empty, "").unwrap();
        assert!(matches!(
            SettingsFile::open(&empty),
            Err(SettingsError::Json(_))
        ));
    }

    #[test]
    fn detects_deleted_files_as_stale() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, "{}").unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        fs::remove_file(&file).unwrap();
        assert!(matches!(editor.save(&values()), Err(SettingsError::Stale)));
    }

    #[test]
    fn invalid_edits_are_safe_and_leave_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source()).unwrap();
        let original = fs::read(&file).unwrap();
        let mut editor = SettingsFile::open(&file).unwrap();
        let mut invalid = editor.values().clone();
        invalid.github_token = TokenEdit::Replace(Secret::new(""));
        assert!(matches!(
            editor.save(&invalid),
            Err(SettingsError::Validation { .. })
        ));
        assert_eq!(fs::read(&file).unwrap(), original);
        invalid = editor.values().clone();
        invalid.repos = vec!["not a repo".into()];
        assert!(matches!(
            editor.save(&invalid),
            Err(SettingsError::Validation { .. })
        ));
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn new_and_existing_files_are_private_and_loadable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing.json");
        fs::write(&existing, r#"{"github":{"token":"old"}}"#).unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();
        let mut existing_editor = SettingsFile::open(&existing).unwrap();
        let existing_values = existing_editor.values().clone();
        existing_editor.save(&existing_values).unwrap();
        assert_eq!(
            fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            crate::config::load(&existing)
                .0
                .github
                .token
                .unwrap()
                .expose(),
            "old"
        );
        let new_file = dir.path().join("new.json");
        let mut new_editor = SettingsFile::open(&new_file).unwrap();
        let mut new_values = values();
        new_values.github_token = TokenEdit::Replace(Secret::new("new"));
        new_editor.save(&new_values).unwrap();
        assert_eq!(
            fs::metadata(&new_file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            crate::config::load(&new_file)
                .0
                .github
                .token
                .unwrap()
                .expose(),
            "new"
        );
    }
}
