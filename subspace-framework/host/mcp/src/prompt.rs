use std::path::Path;

/// Only paths and public connection details; no token value can be supplied.
#[non_exhaustive]
pub struct PromptInput<'a> {
    pub binary_name: &'a str,
    pub display_name: &'a str,
    pub listening_url: Option<&'a str>,
    pub executable: &'a Path,
    pub token_file: Option<&'a Path>,
}

impl<'a> PromptInput<'a> {
    pub fn new(binary_name: &'a str, display_name: &'a str, executable: &'a Path) -> Self {
        Self {
            binary_name,
            display_name,
            executable,
            listening_url: None,
            token_file: None,
        }
    }
}

pub fn agent_prompt(input: PromptInput<'_>) -> String {
    let mut prompt = format!(
        "Add {} as an MCP server named `{}` in your own harness configuration.\n",
        input.display_name, input.binary_name
    );
    match input.listening_url {
        Some(url) => prompt.push_str(&format!("Use Streamable HTTP at `{url}`.\n")),
        None => prompt.push_str("HTTP is off. Use stdio only.\n"),
    }
    prompt.push_str(&format!(
        "Stdio{}: executable `{}`, argument `mcp`.\n",
        if input.listening_url.is_some() {
            " alternative"
        } else {
            " setup"
        },
        input.executable.display()
    ));
    if let Some(file) = input.token_file.filter(|_| input.listening_url.is_some()) {
        prompt.push_str(&format!("For HTTP authentication, read the token from `{}` and send `Authorization: Bearer <token>` with that file content. For a launched `mcp --http` process, set `SUBSPACE_MCP_TOKEN` from the file. Never print the token or store it in your transcript. Stdio itself needs no HTTP token.\n", file.display()));
    }
    prompt.push_str("After setup, list the tools and report them to the user. Write tools run without a Subspace prompt: ask the user before calling a write tool.\n");
    prompt
}

#[cfg(test)]
mod tests {
    use super::{PromptInput, agent_prompt};
    use std::path::Path;

    #[test]
    fn listening_prompt_has_transport_stdio_and_write_consent_without_a_secret() {
        let mut input = PromptInput::new(
            "subspace",
            "Subspace",
            Path::new("/Applications/Subspace.app/Contents/MacOS/subspace"),
        );
        input.listening_url = Some("http://127.0.0.1:7331/mcp");
        input.token_file = Some(Path::new("/test-config/mcp-token"));
        let text = agent_prompt(input);
        for expected in [
            "`subspace`",
            "Streamable HTTP",
            "http://127.0.0.1:7331/mcp",
            "argument `mcp`",
            "/test-config/mcp-token",
            "Authorization: Bearer <token>",
            "SUBSPACE_MCP_TOKEN",
            "list the tools",
            "ask the user",
            "Never print",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        assert!(!text.contains("test-only-secret-value"));
    }

    #[test]
    fn stopped_prompt_gives_stdio_only() {
        let mut input = PromptInput::new("subspace", "Subspace", Path::new("/app/subspace"));
        input.token_file = Some(Path::new("/test-config/mcp-token"));
        let text = agent_prompt(input);
        assert!(text.contains("HTTP is off"));
        assert!(!text.contains("http://"));
        assert!(text.contains("/app/subspace"));
        assert!(!text.contains("mcp --http"));
    }
}
