//! `mcp` options, parsed with the usage spec that help and completion list.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use subspace_core::config::McpSettings;
use subspace_product::{MCP_COMMAND, MCP_USAGE};
use usage::parse::ParseValue;

pub use subspace_core::mcp_token::TOKEN_VARIABLE;

const DEFAULT_PORT: u16 = 7331;
const HTTP_ONLY: [&str; 5] = [
    "bind",
    "token-file",
    "no-auth",
    "allow-origin",
    "allow-host",
];

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    /// Board ids to serve; empty means every selected board.
    pub boards: Vec<String>,
    /// `None` serves stdio.
    pub http: Option<HttpOptions>,
    /// `--attach`: relay stdio to a running app's chat endpoint instead.
    pub attach: Option<Attach>,
}

/// A running app's chat endpoint and its token, from [`TOKEN_VARIABLE`].
#[derive(PartialEq, Eq)]
pub struct Attach {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for Attach {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Attach")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(PartialEq, Eq)]
pub struct HttpOptions {
    pub bind: SocketAddr,
    pub token: Option<String>,
    pub origins: Vec<String>,
    pub hosts: Vec<String>,
}

impl std::fmt::Debug for HttpOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpOptions")
            .field("bind", &self.bind)
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .field("origins", &self.origins)
            .field("hosts", &self.hosts)
            .finish()
    }
}

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Serve(Options),
    /// `--help`: print this page and exit successfully.
    Help(String),
}

/// Parses `args` (after `mcp`). `token_variable` is the value of
/// [`TOKEN_VARIABLE`], passed in so tests do not depend on the process.
#[cfg(test)]
pub fn parse(
    binary: &str,
    args: &[String],
    token_variable: Option<String>,
) -> Result<Parsed, String> {
    let mut settings = McpSettings::default();
    let address = bind_address(subspace_product::MCP_DEFAULT_BIND)?;
    settings.bind = address.ip();
    settings.port = address.port();
    parse_with_settings(
        binary,
        args,
        token_variable,
        &settings,
        Path::new("mcp-token"),
    )
}

pub fn parse_with_settings(
    binary: &str,
    args: &[String],
    token_variable: Option<String>,
    settings: &McpSettings,
    token_file: &Path,
) -> Result<Parsed, String> {
    let spec: usage::Spec = format!("bin {binary:?}\n{MCP_USAGE}\n")
        .parse()
        .map_err(|error| format!("Invalid mcp usage spec: {error}"))?;
    let argv: Vec<String> = [binary, MCP_COMMAND]
        .into_iter()
        .map(str::to_owned)
        .chain(args.iter().cloned())
        .collect();
    let output = usage::parse::Parser::new(&spec)
        .explain(&argv)
        .map_err(|error| error.to_string())?;
    if let Some(page) = output.errors.iter().find_map(help_page) {
        return Ok(Parsed::Help(page));
    }
    if let Some(error) = output.errors.first() {
        return Err(error.to_string());
    }
    let flags = Flags(&output);
    options(&flags, token_variable, settings, token_file).map(Parsed::Serve)
}

fn help_page(error: &usage::error::UsageErr) -> Option<String> {
    match error {
        usage::error::UsageErr::Help(page) => Some(page.clone()),
        _ => None,
    }
}

struct Flags<'a>(&'a usage::parse::ParseOutput);

impl Flags<'_> {
    fn value(&self, name: &str) -> Option<&ParseValue> {
        self.0
            .flags
            .iter()
            .find(|(flag, _)| flag.name == name)
            .map(|(_, value)| value)
    }

    fn set(&self, name: &str) -> bool {
        !matches!(self.value(name), None | Some(ParseValue::Bool(false)))
    }

    fn text(&self, name: &str) -> Option<String> {
        match self.value(name) {
            Some(ParseValue::String(value)) => Some(value.clone()),
            _ => None,
        }
    }

    fn list(&self, name: &str) -> Vec<String> {
        match self.value(name) {
            Some(ParseValue::MultiString(values)) => values.clone(),
            Some(ParseValue::String(value)) => vec![value.clone()],
            _ => Vec::new(),
        }
    }
}

fn options(
    flags: &Flags,
    token_variable: Option<String>,
    settings: &McpSettings,
    token_file: &Path,
) -> Result<Options, String> {
    let boards = flags.list("board");
    if let Some(url) = flags.text("attach") {
        return attach(flags, url, token_variable);
    }
    if !flags.set("http") {
        return match HTTP_ONLY.iter().find(|name| flags.set(name)) {
            Some(name) => Err(format!("--{name} needs --http.")),
            None => Ok(serving(boards, None)),
        };
    }
    let bind = flags
        .text("bind")
        .map_or_else(|| Ok(settings.address()), |value| bind_address(&value))?;
    let token = resolved_token(flags, token_variable, settings, token_file)?;
    check_authentication(&bind, token.is_some(), flags.set("no-auth"))?;
    let http = HttpOptions {
        bind,
        token,
        origins: flags.list("allow-origin"),
        hosts: flags.list("allow-host"),
    };
    Ok(serving(boards, Some(http)))
}

/// Serving the catalog, over stdio when `http` is `None`.
fn serving(boards: Vec<String>, http: Option<HttpOptions>) -> Options {
    Options {
        boards,
        http,
        attach: None,
    }
}

/// `--attach` stands alone and needs the endpoint's token.
fn attach(flags: &Flags, url: String, token: Option<String>) -> Result<Options, String> {
    if flags.set("http") || flags.set("board") {
        return Err("--attach cannot be combined with --http or --board.".into());
    }
    crate::attach::check_url(&url)?;
    let token = token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| format!("--attach needs the endpoint's token in {TOKEN_VARIABLE}."))?;
    Ok(Options {
        boards: Vec::new(),
        http: None,
        attach: Some(Attach {
            url,
            token: token.trim().to_owned(),
        }),
    })
}

fn resolved_token(
    flags: &Flags,
    variable: Option<String>,
    settings: &McpSettings,
    default_file: &Path,
) -> Result<Option<String>, String> {
    let explicit = flags.text("token-file");
    let variable = variable.filter(|value| !value.is_empty());
    if explicit.is_some() || variable.is_some() {
        return token(explicit.as_deref(), variable);
    }
    if settings.require_token && !flags.set("no-auth") {
        return read_token(default_file).map(Some);
    }
    Ok(None)
}

/// `IP:PORT`, `[IPv6]:PORT`, or an IP address on the default port.
pub fn bind_address(text: &str) -> Result<SocketAddr, String> {
    text.parse::<SocketAddr>()
        .or_else(|_| {
            text.parse::<IpAddr>()
                .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
        })
        .map_err(|_| format!("--bind must be an IP address or IP:PORT, got {text:?}."))
}

fn token(file: Option<&str>, variable: Option<String>) -> Result<Option<String>, String> {
    match (file, variable.filter(|value| !value.is_empty())) {
        (Some(_), Some(_)) => Err(format!(
            "Use either --token-file or {TOKEN_VARIABLE}, not both."
        )),
        (Some(path), None) => read_token(Path::new(path)).map(Some),
        (None, Some(value)) => non_empty(value.trim(), TOKEN_VARIABLE).map(Some),
        (None, None) => Ok(None),
    }
}

/// A private, non-empty token file. The token never appears in a message.
pub fn read_token(path: &Path) -> Result<String, String> {
    check_private(path)?;
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("Cannot read the token file {}: {error}", path.display()))?;
    non_empty(text.trim(), "The token file")
}

fn non_empty(token: &str, source: &str) -> Result<String, String> {
    if token.is_empty() {
        Err(format!("{source} holds no token."))
    } else {
        Ok(token.to_owned())
    }
}

fn check_private(path: &Path) -> Result<(), String> {
    let private = subspace_core::private_file::is_owner_only(path)
        .map_err(|error| format!("Cannot read the token file {}: {error}", path.display()))?;
    if private {
        Ok(())
    } else {
        Err(format!(
            "The token file {} is readable by other users; make it private to your user (chmod 600 on macOS and Linux).",
            path.display()
        ))
    }
}

/// A bind beyond loopback needs a token, or an explicit `--no-auth`.
fn check_authentication(bind: &SocketAddr, token: bool, no_auth: bool) -> Result<(), String> {
    match (token, no_auth) {
        (true, true) => Err("--no-auth cannot be combined with a token.".into()),
        (false, false) if !bind.ip().is_loopback() => Err(format!(
            "{bind} is reachable from other machines. Set a token with --token-file or {TOKEN_VARIABLE}, or pass --no-auth to serve read and write tools without authentication."
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{HttpOptions, Options, Parsed, bind_address, parse, read_token};
    use std::net::SocketAddr;

    fn token_defaults() -> (tempfile::TempDir, subspace_core::config::McpSettings) {
        let directory = tempfile::tempdir().unwrap();
        for (file, value) in [
            ("mcp-token", "test-only-settings-token"),
            ("other", "test-only-override-token"),
        ] {
            subspace_core::state::write_atomic(&directory.path().join(file), value).unwrap();
        }
        let mut settings = subspace_core::config::McpSettings::default();
        settings.port = 7400;
        settings.require_token = true;
        (directory, settings)
    }

    #[test]
    fn saved_http_defaults_and_explicit_overrides_follow_precedence() {
        use crate::parse_options_with_settings;
        let (directory, settings) = token_defaults();
        let token_file = directory.path().join("mcp-token");
        let other = directory.path().join("other");
        let parse = |words: &[&str], token| {
            parse_options_with_settings("subspace", &args(words), token, &settings, &token_file)
        };
        let default = parse(&["--http"], None).unwrap().http.unwrap();
        assert_eq!(default.bind, address("127.0.0.1:7400"));
        assert_eq!(default.token.as_deref(), Some("test-only-settings-token"));
        let overridden = parse(
            &[
                "--http",
                "--bind",
                "127.0.0.1:7500",
                "--token-file",
                other.to_str().unwrap(),
            ],
            None,
        )
        .unwrap()
        .http
        .unwrap();
        assert_eq!(overridden.bind, address("127.0.0.1:7500"));
        assert_eq!(
            overridden.token.as_deref(),
            Some("test-only-override-token")
        );
        assert_eq!(
            parse(&["--http"], Some("test-only-environment-token".into()))
                .unwrap()
                .http
                .unwrap()
                .token
                .as_deref(),
            Some("test-only-environment-token")
        );
        assert!(
            parse(
                &["--http", "--token-file", other.to_str().unwrap()],
                Some("test-only-environment-token".into())
            )
            .is_err()
        );
        assert!(parse(&[], None).unwrap().http.is_none());
        assert!(
            parse(&["--http", "--no-auth"], None)
                .unwrap()
                .http
                .unwrap()
                .token
                .is_none()
        );
        assert!(!format!("{default:?}").contains("test-only-settings-token"));
        std::fs::remove_file(&token_file).unwrap();
        assert!(parse(&["--http"], None).is_err());
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn serve(words: &[&str], variable: Option<&str>) -> Result<Options, String> {
        match parse("subspace", &args(words), variable.map(str::to_owned))? {
            Parsed::Serve(options) => Ok(options),
            Parsed::Help(_) => Err("help".into()),
        }
    }

    fn address(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn stdio_is_the_default_and_boards_repeat() {
        assert_eq!(
            serve(&[], None).unwrap(),
            Options {
                boards: Vec::new(),
                http: None,
                attach: None,
            }
        );
        let options = serve(&["--board", "github", "--board", "irc"], None).unwrap();
        assert_eq!(options.boards, ["github", "irc"]);
        for http_only in [
            &["--bind", "127.0.0.1"][..],
            &["--no-auth"],
            &["--allow-origin", "https://a.example"],
            &["--allow-host", "a.example"],
            &["--token-file", "token"],
        ] {
            let error = serve(http_only, None).unwrap_err();
            assert!(error.contains("needs --http"), "{error}");
        }
    }

    #[test]
    fn attach_relays_to_a_loopback_endpoint_with_the_token_from_the_environment() {
        let url = "http://127.0.0.1:51234/mcp";
        let options = serve(&["--attach", url], Some("test-only-chat-token")).unwrap();
        let attach = options.attach.unwrap();
        assert_eq!(
            (attach.url.as_str(), attach.token.as_str()),
            (url, "test-only-chat-token")
        );
        assert!(!format!("{attach:?}").contains("test-only-chat-token"));
        let missing = serve(&["--attach", url], None).unwrap_err();
        assert!(missing.contains("SUBSPACE_MCP_TOKEN"), "{missing}");
        for combined in [&["--http"][..], &["--board", "irc"]] {
            let mut words = vec!["--attach", url];
            words.extend_from_slice(combined);
            let error = serve(&words, Some("token")).unwrap_err();
            assert!(error.contains("cannot be combined"), "{error}");
        }
        let remote = serve(&["--attach", "http://192.0.2.4:1/mcp"], Some("token"));
        assert!(remote.unwrap_err().contains("loopback"));
    }

    #[test]
    fn http_defaults_to_loopback_without_a_token() {
        let options = serve(&["--http"], None).unwrap();
        assert_eq!(
            options.http,
            Some(HttpOptions {
                bind: address("127.0.0.1:7331"),
                token: None,
                origins: Vec::new(),
                hosts: Vec::new(),
            })
        );
        let options = serve(
            &[
                "--http",
                "--bind",
                "[::1]:9000",
                "--allow-origin",
                "https://a.example:443",
                "--allow-host",
                "a.example",
            ],
            Some("  secret  "),
        )
        .unwrap()
        .http
        .unwrap();
        assert_eq!(options.bind, address("[::1]:9000"));
        assert_eq!(options.token.as_deref(), Some("secret"));
        assert_eq!(options.origins, ["https://a.example:443"]);
        assert_eq!(options.hosts, ["a.example"]);
    }

    #[test]
    fn a_bind_beyond_loopback_needs_a_token_or_an_explicit_opt_out() {
        let error = serve(&["--http", "--bind", "0.0.0.0"], None).unwrap_err();
        assert!(error.contains("--no-auth"), "{error}");
        let open = serve(&["--http", "--bind", "192.0.2.4:8000", "--no-auth"], None).unwrap();
        assert_eq!(open.http.unwrap().bind, address("192.0.2.4:8000"));
        let tokened = serve(&["--http", "--bind", "0.0.0.0"], Some("secret")).unwrap();
        assert_eq!(tokened.http.unwrap().bind, address("0.0.0.0:7331"));
        assert!(serve(&["--http", "--no-auth"], Some("secret")).is_err());
        assert!(serve(&["--http", "--bind", "not-an-ip"], None).is_err());
        assert!(bind_address("10.0.0.1:99999").is_err());
    }

    #[test]
    fn tokens_come_from_one_private_non_empty_source() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("token");
        std::fs::File::create(&file).unwrap();
        subspace_core::private_file::set_owner_only(&file).unwrap();
        std::fs::write(&file, "  dummy-test-only-token\n").unwrap();
        assert!(subspace_core::private_file::is_owner_only(&file).unwrap());
        assert_eq!(read_token(&file).unwrap(), "dummy-test-only-token");
        let path = file.to_str().unwrap();
        assert!(serve(&["--http", "--token-file", path], Some("other")).is_err());
        assert_eq!(
            serve(&["--http", "--token-file", path], None)
                .unwrap()
                .http
                .unwrap()
                .token
                .as_deref(),
            Some("dummy-test-only-token")
        );
        std::fs::write(&file, " \n").unwrap();
        assert!(read_token(&file).unwrap_err().contains("no token"));
        assert!(serve(&["--http"], Some("   ")).is_err());
        assert!(
            serve(&["--http"], Some(""))
                .unwrap()
                .http
                .unwrap()
                .token
                .is_none(),
            "an empty variable is unset"
        );
        assert!(read_token(&directory.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_token_file_others_can_read_is_refused_without_echoing_the_token() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("token");
        std::fs::write(&file, "visible-secret").unwrap();
        set_mode(&file, 0o644);
        let error = read_token(&file).unwrap_err();
        assert!(error.contains("chmod 600"), "{error}");
        assert!(!error.contains("visible-secret"));
    }

    #[test]
    fn help_and_unknown_options_are_reported() {
        let Parsed::Help(page) = parse("subspace", &args(&["--help"]), None).unwrap() else {
            panic!("expected help")
        };
        for flag in ["--board", "--http", "--bind", "--token-file", "--no-auth"] {
            assert!(page.contains(flag), "{flag}");
        }
        assert!(parse("subspace", &args(&["--unknown"]), None).is_err());
    }

    #[cfg(unix)]
    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}
