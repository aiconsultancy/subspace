//! The `mcp` root command, which every product mounts before its boards'
//! commands. The usage spec is shared, so the MCP server parses the same
//! options that help and shell completion list.

/// The reserved root command name. No board or alias may use it.
pub const MCP_COMMAND: &str = "mcp";

/// The default Streamable HTTP address.
pub const MCP_DEFAULT_BIND: &str = "127.0.0.1:7331";

/// usage.jdx.dev KDL for the `mcp` command.
pub const MCP_USAGE: &str = r#"cmd "mcp" effect="write" help="Serve the selected boards' agent tools to other harnesses over MCP (stdio by default)" {
  flag "--board <ID>" var=#true help="Serve only this board's tools; repeat for more boards"
  flag "--http" help="Serve Streamable HTTP on /mcp instead of stdio"
  flag "--bind <ADDRESS>" help="HTTP address, IP or IP:PORT (overrides mcp.bind and mcp.port; default 127.0.0.1:7331)"
  flag "--token-file <PATH>" help="Require the bearer token in PATH (or set SUBSPACE_MCP_TOKEN); overrides the settings token file"
  flag "--no-auth" help="Allow an HTTP bind beyond loopback without a token"
  flag "--allow-origin <ORIGIN>" var=#true help="Also accept this browser Origin, such as https://app.example.com:443; repeat for more"
  flag "--allow-host <HOST>" var=#true help="Also accept this Host header value; repeat for more"
  flag "--attach <URL>" help="Relay stdio to the running app's tools for one chat at this loopback URL (token in SUBSPACE_MCP_TOKEN); chat starts it for agents without HTTP MCP"
}"#;
