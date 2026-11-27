//! The config file (REQ-20): `config.json` in the platform config directory.
//! The file is optional. Parsing never fails: an invalid value is reported
//! as a [`Notice`] and replaced by its default. Tokens in a file that other
//! users can read or write are ignored (REQ-23).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Secret,
    features::{FeatureSpec, FeatureStage},
};
mod acp;
mod board_files;
mod mcp;
mod notice;
mod parser;
pub mod web_boards;

pub use acp::{AcpAgent, AcpConfig};
pub use board_files::board_file;
pub use mcp::{McpFieldError, McpSettings};
pub use notice::{Notice, NoticeKind};

#[derive(Debug, Clone, PartialEq, Eq)]
// qual:allow(srp, god_struct) reason: "The parsed product configuration groups independent service credentials and board preferences; consumers intentionally read only their own fields."
pub struct Config {
    /// The `terminal` object as the file has it. The terminal panel reads
    /// its own keys, such as `fontFamily` and `fontSize`.
    pub terminal: serde_json::Map<String, Value>,
    pub board_order: Vec<String>,
    pub auto_refresh: bool,
    pub chat: ChatConfig,
    pub mcp: McpSettings,
    pub web: WebConfig,
    pub github: GitHub,
    /// `owner/name`, unique without regard to case, in file order.
    pub repos: Vec<String>,
    /// Explicit board choices. Missing ids use the current product
    /// descriptor's default; ids the product does not build stay unknown.
    pub boards: BTreeMap<String, bool>,
    /// Saved choices for registered app and board features. Valid unknown
    /// feature ids remain here for forward compatibility.
    pub features: BTreeMap<String, bool>,
    /// Native boards' settings from their own files ([`board_file`]),
    /// keyed by board id. Only [`load_with_boards`] reads them.
    pub board_settings: BTreeMap<String, serde_json::Map<String, Value>>,
    pub web_boards: Vec<web_boards::WebBoard>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal: serde_json::Map::new(),
            board_order: Vec::new(),
            auto_refresh: true,
            chat: ChatConfig::default(),
            mcp: McpSettings::default(),
            web: WebConfig::default(),
            github: GitHub::default(),
            repos: Vec::new(),
            boards: BTreeMap::new(),
            features: BTreeMap::new(),
            board_settings: BTreeMap::new(),
            web_boards: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatEngine {
    #[default]
    Off,
    Copilot,
    Pi,
    /// An Agent Client Protocol agent, chosen by [`ChatConfig::acp`].
    Acp,
}

impl ChatEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Copilot => "Copilot",
            Self::Pi => "Pi",
            Self::Acp => "ACP agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatConfig {
    pub engine: ChatEngine,
    pub acp: AcpConfig,
}

/// Administrator-controlled web research boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WebConfig {
    /// Exact hosts or one-label wildcards such as `*.example.com`.
    pub allowed_hosts: Vec<String>,
    /// Allowed hosts that do not need per-call approval.
    pub silent_allowed_hosts: Vec<String>,
    pub search: WebSearchConfig,
}

/// An optional external search provider. Subspace supplies no provider itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WebSearchConfig {
    pub provider: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHub {
    pub token: Option<Secret>,
}

impl Config {
    /// Whether the product may show and serve the board `id`. Every board of
    /// the product is active unless the config file sets it to `false`.
    pub fn board_enabled(&self, id: &str) -> bool {
        self.boards.get(id).copied().unwrap_or(true)
    }

    /// Resolves a product's default while preserving an explicit config choice.
    pub fn board_enabled_or(&self, id: &str, default_enabled: bool) -> bool {
        self.boards.get(id).copied().unwrap_or(default_enabled)
    }

    /// Resolves one registered feature's lifecycle and toggle policy.
    pub fn feature_enabled(&self, spec: &FeatureSpec) -> bool {
        if !crate::features::valid_id(spec.id) {
            return false;
        }
        if let Some(owner_and_feature) = spec.id.strip_prefix("board.") {
            let owner = owner_and_feature
                .split_once('.')
                .map(|(owner, _)| owner)
                .unwrap_or_default();
            if !self.boards.get(owner).copied().unwrap_or(true) {
                return false;
            }
        }
        match (spec.stage, spec.toggleable) {
            (FeatureStage::Removed, _) | (FeatureStage::Experimental, false) => false,
            (FeatureStage::Experimental, true) => {
                self.features.get(spec.id).copied().unwrap_or(false)
            }
            (FeatureStage::Stable, false) => spec.default_enabled,
            (FeatureStage::Stable, true) => self
                .features
                .get(spec.id)
                .copied()
                .unwrap_or(spec.default_enabled),
        }
    }

    fn has_tokens(&self) -> bool {
        self.github.token.is_some()
            || self
                .web_boards
                .iter()
                .any(|board| !board.secrets.is_empty())
    }

    fn clear_tokens(&mut self) {
        self.github.token = None;
        self.web_boards
            .iter_mut()
            .for_each(|board| board.secrets.clear());
    }
}

/// `<directory>/config.json` in the platform config directory, where the
/// directory is [`crate::state::directory_name`].
pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(crate::state::directory_name()).join("config.json"))
}

/// Where the config file is, for messages that say where to set a value:
/// its path, or "the config file" when there is no config directory.
pub fn place() -> String {
    path().map_or_else(
        || "the config file".into(),
        |path| path.display().to_string(),
    )
}

/// Reads the config file. A missing file gives the defaults and no notice.
pub fn load(file: &Path) -> (Config, Vec<Notice>) {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Default::default(),
        Err(error) => {
            let notice = warning(file, format!("cannot read the file: {error}"));
            return (Config::default(), vec![notice]);
        }
    };
    let (mut config, mut notices) = parse(&text, file);
    if config.has_tokens() && open_to_others(file) {
        config.clear_tokens();
        notices.push(warning(
            file,
            format!(
                "other users can read or write this file, so Subspace ignores its tokens; \
                 run `chmod 600 {}`",
                file.display()
            ),
        ));
    }
    (config, notices)
}

/// Reads the config file and the settings files of the boards `ids`.
pub fn load_with_boards<'a>(
    file: &Path,
    ids: impl IntoIterator<Item = &'a str>,
) -> (Config, Vec<Notice>) {
    let (mut config, mut notices) = load(file);
    let (boards, board_notices) = board_files::load(file, ids);
    config.board_settings = boards;
    notices.extend(board_notices);
    (config, notices)
}

fn warning(file: &Path, message: impl Into<String>) -> Notice {
    Notice {
        kind: NoticeKind::Warning,
        file: file.into(),
        key: String::new(),
        message: message.into(),
    }
}

/// True when group or others may read or write the file (REQ-23).
#[cfg(unix)]
fn open_to_others(file: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // An unreadable mode counts as open, so tokens are ignored rather than trusted.
    fs::metadata(file).map_or(true, |meta| meta.permissions().mode() & 0o066 != 0)
}

#[cfg(not(unix))]
fn open_to_others(_file: &Path) -> bool {
    false
}

/// Parses the text of the config file. `file` names the file in notices.
/// Settings saved under a Space's former id are read as the Space's own (see
/// [`crate::former_ids`]).
pub fn parse(text: &str, file: &Path) -> (Config, Vec<Notice>) {
    parse_with(text, file, &crate::former_ids::installed())
}

/// As [`parse`], with the former ids `renames`.
pub(crate) fn parse_with(
    text: &str,
    file: &Path,
    renames: &crate::former_ids::Renames,
) -> (Config, Vec<Notice>) {
    parser::parse(text, file, renames)
}

/// `owner/name` as `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`.
pub fn is_repo(value: &str) -> bool {
    let part = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    value
        .split_once('/')
        .is_some_and(|(owner, name)| part(owner) && part(name))
}

/// How much of an invalid value a notice repeats.
const MAX_NOTICE_CHARS: usize = 80;

/// A short JSON rendering of an invalid value for a notice.
fn show(value: &Value) -> String {
    value.to_string().chars().take(MAX_NOTICE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ChatEngine, Config, is_repo, parse};
    use crate::features::{FeatureSpec, FeatureStage};

    fn parsed(text: &str) -> (Config, Vec<String>) {
        let (config, notices) = parse(text, Path::new("/c/config.json"));
        (config, notices.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn a_full_file_parses() {
        let (config, notices) = parsed(
            r#"{
              "github": {"token": " gh "},
              "chat": {"engine": "pi"},
              "terminal": {"fontFamily": "Menlo"},
              "boardOrder": ["github", "github-copilot-agents"],
              "autoRefresh": false,
              "repos": ["o/b", "O/A"],
              "unknown": 1
            }"#,
        );
        assert_eq!(notices, Vec::<String>::new());
        assert_eq!(config.github.token.as_ref().map(|t| t.expose()), Some("gh"));
        assert_eq!(config.repos, ["o/b", "O/A"]);
        assert_eq!(config.chat.engine, ChatEngine::Pi);
        assert_eq!(config.terminal["fontFamily"], "Menlo");
        assert_eq!(config.board_order, ["github", "github-copilot-agents"]);
        assert!(!config.auto_refresh);
    }

    #[test]
    fn invalid_json_gives_the_defaults_and_a_notice_with_the_path() {
        let (config, notices) = parsed("{ nope");
        assert_eq!(config, Config::default());
        assert_eq!(notices.len(), 1);
        assert!(
            notices[0].starts_with("/c/config.json: invalid JSON"),
            "{notices:?}"
        );
    }

    #[test]
    fn invalid_values_keep_their_defaults_and_name_their_key() {
        let (config, notices) =
            parsed(r#"{"github": [], "web": {"search": {"endpoint": "http://search"}}}"#);
        assert_eq!(config, Config::default());
        assert_eq!(
            notices,
            [
                "/c/config.json: web.search.endpoint: must be a plain https URL \
                 (no credentials, query or fragment)",
                "/c/config.json: github: expected an object; using the defaults",
            ]
        );
    }

    #[test]
    fn chat_engine_defaults_to_off_and_preserves_explicit_choices() {
        assert_eq!(parsed("{}").0.chat.engine, ChatEngine::Off);
        for engine in [
            ChatEngine::Off,
            ChatEngine::Copilot,
            ChatEngine::Pi,
            ChatEngine::Acp,
        ] {
            let config = serde_json::json!({ "chat": { "engine": engine } });
            let (config, notices) = parsed(&config.to_string());
            assert_eq!(config.chat.engine, engine);
            assert!(notices.is_empty());
        }
        assert_eq!(ChatEngine::Copilot.label(), "Copilot");
        assert_eq!(ChatEngine::Pi.label(), "Pi");
    }

    #[test]
    fn web_research_is_disabled_by_default_and_parses_safe_boundaries() {
        let defaults = parsed("{}").0;
        assert!(defaults.web.allowed_hosts.is_empty());
        assert_eq!(defaults.web.search.provider, None);

        let (config, notices) = parsed(
            r#"{"web":{"allowedHosts":["example.com","*.example.org"],"silentAllowedHosts":["docs.example.org"],"search":{"provider":"fixture","endpoint":"https://search.example.com/api"}}}"#,
        );
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(config.web.allowed_hosts, ["example.com", "*.example.org"]);
        assert_eq!(config.web.silent_allowed_hosts, ["docs.example.org"]);
        assert_eq!(config.web.search.provider.as_deref(), Some("fixture"));
        assert_eq!(
            config.web.search.endpoint.as_deref(),
            Some("https://search.example.com/api")
        );
    }

    #[test]
    fn the_terminal_object_is_kept_as_written_and_a_non_object_is_a_notice() {
        let (config, notices) = parsed(r#"{"terminal":{"fontSize":13,"other":true}}"#);
        assert_eq!(config.terminal["fontSize"], 13);
        assert_eq!(config.terminal["other"], true);
        assert!(notices.is_empty());
        let (config, notices) = parsed(r#"{"terminal":"bottom"}"#);
        assert!(config.terminal.is_empty());
        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn board_order_and_auto_refresh_have_safe_defaults_and_parse() {
        let defaults = parsed("{}").0;
        assert!(defaults.auto_refresh);
        assert!(defaults.board_order.is_empty());

        let (config, notices) =
            parsed(r#"{"boardOrder":["irc",7,"irc","github"],"autoRefresh":false}"#);
        assert_eq!(notices, Vec::<String>::new());
        assert_eq!(config.board_order, ["irc", "irc", "github"]);
        assert!(!config.auto_refresh);
    }

    #[test]
    fn malformed_chat_settings_warn_and_leave_chat_off() {
        let (config, notices) = parsed(r#"{"chat":{"engine":"other"}}"#);
        assert_eq!(config.chat.engine, ChatEngine::Off);
        assert_eq!(
            notices,
            [
                "/c/config.json: chat.engine: expected \"off\", \"copilot\", \"pi\" or \"acp\", got \"other\"; using the default"
            ]
        );

        let (config, notices) = parsed(r#"{"chat":[]}"#);
        assert_eq!(config.chat.engine, ChatEngine::Off);
        assert_eq!(
            notices,
            ["/c/config.json: chat: expected an object; using the defaults"]
        );
    }

    #[test]
    fn empty_tokens_count_as_unset() {
        for text in [
            r#"{"github": {"token": ""}}"#,
            r#"{"github": {"token": null}}"#,
        ] {
            let (config, notices) = parsed(text);
            assert!(notices.is_empty());
            assert_eq!(config.github.token, None);
        }
    }

    #[test]
    fn repos_follow_the_my_repos_rules() {
        let (config, notices) = parsed(
            r#"{
              "repos": ["Org/One", "org/one", "bad", 5, "org/two"]
            }"#,
        );
        assert_eq!(config.repos, ["Org/One", "org/two"]);
        let keys: Vec<&str> = notices
            .iter()
            .map(|notice| notice.split(": ").nth(1).unwrap_or_default())
            .collect();
        assert_eq!(keys, ["repos[2]", "repos[3]"]);
    }

    #[test]
    fn repo_pattern() {
        assert!(is_repo("example-org/example.app_1"));
        for bad in ["", "a", "a/", "/b", "a/b/c", "a b/c", "a/b?"] {
            assert!(!is_repo(bad), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn tokens_in_a_file_others_can_read_are_ignored() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        use super::load;

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, r#"{"github": {"token": "gh"}, "repos": ["o/a"]}"#).unwrap();

        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        let (config, notices) = load(&file);
        assert_eq!(config.github.token, None);
        assert_eq!(config.repos, ["o/a"]);
        assert_eq!(notices.len(), 1);
        assert!(notices[0].to_string().contains("chmod 600"));
        assert!(notices[0].file == file);

        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let (config, notices) = load(&file);
        assert!(notices.is_empty());
        assert!(config.github.token.is_some());
    }

    #[test]
    fn every_board_is_enabled_until_the_file_turns_one_off() {
        let (config, notices) = parsed(r#"{"boards": {"irc": false, "github": true}}"#);
        assert!(notices.is_empty());
        assert!(!config.board_enabled("irc"));
        assert!(config.board_enabled("github"));
        assert!(
            config.board_enabled("actions"),
            "an unnamed board is active"
        );
        assert!(Config::default().board_enabled("irc"));
    }

    #[test]
    fn board_defaults_apply_only_when_the_file_has_no_explicit_choice() {
        let (config, notices) = parsed(r#"{"boards": {"irc": true}}"#);
        assert!(notices.is_empty());
        assert!(!config.board_enabled_or("actions", false));
        assert!(config.board_enabled_or("actions", true));
        assert!(config.board_enabled_or("irc", false));
    }

    #[test]
    fn an_unusable_board_entry_keeps_the_product_default_and_warns() {
        let (config, notices) = parsed(
            r#"{"boards": {"irc": "false", "github": 0, "actions": null, " ": true,
                          "github-copilot-agents": false}}"#,
        );
        assert!(config.board_enabled("irc"), "a string is not coerced");
        assert!(config.board_enabled("github"));
        assert!(
            config.board_enabled("actions"),
            "null leaves the board active"
        );
        assert!(!config.board_enabled("github-copilot-agents"));
        assert!(!config.board_enabled_or("irc", false));
        assert!(config.board_enabled_or("irc", true));
        assert!(!config.board_enabled_or("github", false));
        assert!(config.board_enabled_or("github", true));
        assert!(!config.board_enabled_or("actions", false));
        assert!(config.board_enabled_or("actions", true));
        let mut notices = notices;
        notices.sort();
        assert_eq!(
            notices,
            [
                "/c/config.json: boards.actions: expected true or false, got null; \
                 the product default applies",
                "/c/config.json: boards.github: expected true or false, got 0; \
                 the product default applies",
                "/c/config.json: boards.irc: expected true or false, got \"false\"; \
                 the product default applies",
                "/c/config.json: boards: expected a board id; ignored",
            ]
        );
    }

    #[test]
    fn feature_policy_preserves_choices_through_graduation_and_removal() {
        let mut config = Config::default();
        config.features.insert("app.preview".into(), false);
        let experimental = FeatureSpec {
            id: "app.preview",
            title: "Preview",
            description: "Preview behavior",
            stage: FeatureStage::Experimental,
            default_enabled: true,
            toggleable: true,
        };
        let stable = FeatureSpec {
            stage: FeatureStage::Stable,
            ..experimental
        };
        let removed = FeatureSpec {
            stage: FeatureStage::Removed,
            ..experimental
        };

        assert!(!config.feature_enabled(&experimental));
        assert!(!config.feature_enabled(&stable));
        assert!(!config.feature_enabled(&removed));
        assert!(!config.features[experimental.id]);
    }

    #[test]
    fn feature_defaults_and_owners_are_isolated() {
        let mut config = Config::default();
        config.features.insert("app.preview".into(), true);
        config.features.insert("board.github.preview".into(), false);
        config.features.insert("board.irc.preview".into(), true);
        let app = FeatureSpec {
            id: "app.preview",
            title: "App preview",
            description: "App preview behavior",
            stage: FeatureStage::Experimental,
            default_enabled: false,
            toggleable: true,
        };
        let board = FeatureSpec {
            id: "board.github.preview",
            title: "Board preview",
            description: "Board preview behavior",
            ..app
        };
        let stable_fixed = FeatureSpec {
            id: "board.irc.fixed",
            title: "Fixed",
            description: "Fixed stable behavior",
            stage: FeatureStage::Stable,
            default_enabled: true,
            toggleable: false,
        };
        let irc = FeatureSpec {
            id: "board.irc.preview",
            title: "IRC preview",
            description: "IRC preview behavior",
            ..app
        };
        config.features.insert(stable_fixed.id.into(), false);
        config.boards.insert("github".into(), false);

        assert!(config.feature_enabled(&app));
        assert!(!config.feature_enabled(&board));
        assert!(config.feature_enabled(&irc));
        assert!(config.feature_enabled(&stable_fixed));
        assert!(!Config::default().feature_enabled(&app));
        assert!(!config.features[board.id]);
        assert!(config.features[irc.id]);
    }

    #[test]
    fn disabling_a_board_suspends_but_does_not_delete_its_feature_choices() {
        let feature = FeatureSpec {
            id: "board.github.preview",
            title: "GitHub preview",
            description: "GitHub preview behavior",
            stage: FeatureStage::Experimental,
            default_enabled: false,
            toggleable: true,
        };
        let malformed = FeatureSpec {
            id: "board.github",
            ..feature
        };
        let mut config = Config::default();
        config.features.insert(feature.id.into(), true);
        config.features.insert(malformed.id.into(), true);
        config.boards.insert("github".into(), false);

        assert!(!config.feature_enabled(&feature));
        assert!(!config.feature_enabled(&malformed));
        assert!(config.features[feature.id]);

        config.boards.remove("github");
        assert!(config.feature_enabled(&feature));
        assert!(!config.feature_enabled(&malformed));
    }

    #[test]
    fn malformed_feature_entries_warn_and_valid_unknown_flags_survive() {
        let (config, notices) = parsed(
            r#"{"features":{"app.pi-chat":true,"board.future.deep.option":false,"bad":true,"app.wrong":"yes","board..empty":false}}"#,
        );

        assert_eq!(config.features.len(), 2);
        assert!(config.features["app.pi-chat"]);
        assert!(!config.features["board.future.deep.option"]);
        assert_eq!(notices.len(), 3);
        assert!(notices.iter().any(|notice| notice.contains("features.bad")));
        assert!(
            notices
                .iter()
                .any(|notice| notice.contains("features.app.wrong"))
        );
        assert!(notices.iter().any(|notice| notice.contains("board..empty")));

        let (config, notices) = parsed(r#"{"features":[]}"#);
        assert!(config.features.is_empty());
        assert_eq!(
            notices,
            ["/c/config.json: features: expected an object; using the defaults"]
        );
    }

    #[test]
    fn a_boards_value_that_is_not_an_object_keeps_every_board() {
        let (config, notices) = parsed(r#"{"boards": ["irc"]}"#);
        assert!(config.boards.is_empty());
        assert!(config.board_enabled("irc"));
        assert_eq!(
            notices,
            ["/c/config.json: boards: expected an object; using the defaults"]
        );
    }

    #[test]
    fn a_missing_file_gives_the_defaults_without_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let (config, notices) = super::load(&dir.path().join("config.json"));
        assert_eq!(config, Config::default());
        assert!(notices.is_empty());
    }
}
