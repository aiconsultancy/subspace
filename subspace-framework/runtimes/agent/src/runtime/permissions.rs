//! Copilot's permission requests: credential paths and sandbox bypass are
//! rejected, reads inside Subspace's workspace are allowed, and anything else
//! asks Zack with a readable summary.
use super::decisions::Decisions;
use super::redact;
use async_trait::async_trait;
use github_copilot_sdk::handler::{PermissionHandler, PermissionResult};
use github_copilot_sdk::types::{
    PermissionRequestData, PermissionRequestKind, RequestId, SessionId,
};
use std::path::{Path, PathBuf};

/// Lines of a diff or file body kept in an approval card.
const SUMMARY_LINES: usize = 20;
/// Characters kept per summarised field.
const SUMMARY_CHARS: usize = 2000;

fn permission_strings(value: &serde_json::Value) -> Vec<&str> {
    let mut pending = vec![value];
    let mut strings = Vec::new();
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::String(text) => strings.push(text.as_str()),
            serde_json::Value::Array(values) => pending.extend(values),
            serde_json::Value::Object(values) => pending.extend(values.values()),
            _ => {}
        }
    }
    strings
}

fn forbidden_permission_path(value: &serde_json::Value) -> bool {
    permission_strings(value)
        .into_iter()
        .any(credential_path_text)
}

fn credential_path_text(text: &str) -> bool {
    text.split(|character: char| {
        character.is_whitespace() || matches!(character, '\'' | '"' | ';' | '=')
    })
    .any(credential_path)
}

fn credential_path(text: &str) -> bool {
    let normalized = text.replace('\\', "/");
    let path = std::path::Path::new(&normalized);
    super::files::sensitive_path(path)
        || path
            .canonicalize()
            .is_ok_and(|path| super::files::sensitive_path(&path))
}

/// The nested request the CLI sends, or the payload itself for an older shape.
fn permission_request(extra: &serde_json::Value) -> &serde_json::Value {
    extra.get("permissionRequest").unwrap_or(extra)
}

/// The path a read asks for. `resolvedPath` is the runtime's own canonical
/// answer; `path` is what it shows the user.
fn requested_path(request: &serde_json::Value) -> Option<&str> {
    ["resolvedPath", "path"]
        .into_iter()
        .filter_map(|key| request.get(key)?.as_str())
        .find(|value| !value.is_empty())
}

/// Whether a read stays inside `workspace`. The path is canonicalised, so a
/// symlink pointing out of the workspace does not pass. A path that cannot be
/// canonicalised, such as one that does not exist, is treated as outside.
fn read_inside_workspace(workspace: &Path, request: &serde_json::Value) -> bool {
    let Some(path) = requested_path(request) else {
        return false;
    };
    let Ok(resolved) = PathBuf::from(path).canonicalize() else {
        return false;
    };
    resolved.starts_with(workspace)
}

/// A read of Subspace's own workspace needs no prompt, unless enterprise
/// policy asks for a human decision. `view`, `glob` and `grep` run in the CLI
/// process rather than the sandbox, so a read of any other path is shown.
/// Credential paths are rejected before this check.
fn reads_without_asking(workspace: &Path, data: &PermissionRequestData) -> bool {
    if data.managed_approval_required == Some(true) {
        return false;
    }
    native_canvas_read(data)
        || (data.kind == Some(PermissionRequestKind::Read)
            && read_inside_workspace(workspace, permission_request(&data.extra)))
}

// Only the explicit native provider is available: external discovery and MCP
// remain blocked. Its callback validates input and performs no host operations.
fn native_canvas_read(data: &PermissionRequestData) -> bool {
    let request = permission_request(&data.extra);
    data.kind == Some(PermissionRequestKind::CustomTool)
        && matches!(
            request.get("toolName").and_then(serde_json::Value::as_str),
            Some("open_canvas" | "list_canvas_capabilities")
        )
}

/// The first [`SUMMARY_LINES`] lines, bounded by [`SUMMARY_CHARS`], with a
/// note when something was left out.
fn shortened(text: &str) -> String {
    let mut kept: String = text
        .lines()
        .take(SUMMARY_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let cut_lines = text.lines().count().saturating_sub(SUMMARY_LINES);
    if kept.chars().count() > SUMMARY_CHARS {
        kept = kept.chars().take(SUMMARY_CHARS).collect();
        return format!(
            "{kept}\n… shortened; {} characters in all",
            text.chars().count()
        );
    }
    if cut_lines > 0 {
        return format!("{kept}\n… {cut_lines} more lines");
    }
    kept
}

/// One readable line per field the CLI sends for this kind of request: the
/// full shell command, the file and its diff, the URL, the agent prompt.
/// Unknown shapes fall back to the JSON.
fn permission_summary(extra: &serde_json::Value) -> String {
    const PLAIN: [(&str, &str); 11] = [
        ("kind", "Kind"),
        ("toolName", "Tool"),
        ("toolTitle", "Tool title"),
        ("serverName", "Server"),
        ("intention", "Why"),
        ("fullCommandText", "Command"),
        ("path", "Path"),
        ("fileName", "File"),
        ("url", "URL"),
        ("redirectedFrom", "Redirected from"),
        ("warning", "Warning"),
    ];
    let request = permission_request(extra);
    let mut lines: Vec<String> = PLAIN
        .iter()
        .filter_map(|(key, label)| {
            let value = request.get(*key)?.as_str()?;
            (!value.is_empty()).then(|| format!("{label}: {value}"))
        })
        .collect();
    lines.extend(change_lines(request));
    lines.extend(prompt_line(request));
    if lines.is_empty() {
        return format!("Copilot requests permission for:\n{extra}");
    }
    lines.join("\n")
}

/// What a create or edit would do: the diff, or the size of a new file.
fn change_lines(request: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(diff) = request.get("diff").and_then(serde_json::Value::as_str)
        && !diff.is_empty()
    {
        lines.push(format!("Change:\n{}", shortened(diff)));
    }
    if let Some(contents) = request
        .get("newFileContents")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!(
            "New file: {} characters, {} lines",
            contents.chars().count(),
            contents.lines().count()
        ));
    }
    lines
}

/// The instruction a sub-agent would be given. `task` and `write_agent` carry
/// it in the tool arguments, which the CLI sends for custom-tool and MCP
/// prompts.
fn prompt_line(request: &serde_json::Value) -> Option<String> {
    let args = request.get("args")?;
    let prompt = ["prompt", "instructions", "description", "content"]
        .into_iter()
        .filter_map(|key| args.get(key)?.as_str())
        .find(|value| !value.is_empty())?;
    Some(format!("Prompt:\n{}", shortened(prompt)))
}

/// An escalation request: the tool asks to run outside the sandbox, or to
/// keep it only in a recording mode, or for a policy edit that widens it.
/// The flag names themselves contain "sandbox", so the fields are read rather
/// than the JSON text: a plain shell command carries `requestSandboxBypass:
/// false` and must still be offered for approval.
fn requests_sandbox_escalation(extra: &serde_json::Value) -> bool {
    let request = permission_request(extra);
    let flagged = |key: &str| request.get(key).and_then(serde_json::Value::as_bool) == Some(true);
    flagged("requestSandboxBypass")
        || flagged("requestSandboxPermissive")
        || request
            .get("sandboxPathGrant")
            .is_some_and(|grant| !grant.is_null())
        || request
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.to_ascii_lowercase().contains("sandbox"))
}

#[async_trait]
impl PermissionHandler for Decisions {
    async fn handle(
        &self,
        session: SessionId,
        id: RequestId,
        data: PermissionRequestData,
    ) -> PermissionResult {
        // A generic approval must never authorize a sandbox opt-out.
        if forbidden_permission_path(&data.extra) {
            return PermissionResult::reject(Some(
                "Host credential paths cannot be accessed by Copilot.".into(),
            ));
        }
        if requests_sandbox_escalation(&data.extra) {
            return PermissionResult::reject(Some(
                "Sandbox bypass requires a verified explicit mode change.".into(),
            ));
        }
        if reads_without_asking(&self.workspace, &data) {
            return PermissionResult::approve_once();
        }
        // A request from a chat Subspace has closed has nobody to ask.
        let Some(chat) = self.chat_of(&session) else {
            return PermissionResult::reject(Some("That chat is no longer open.".into()));
        };
        let summary = redact(&permission_summary(&data.extra));
        if self
            .approval(chat, format!("sdk-{}", id.into_inner()), summary)
            .await
            .unwrap_or(false)
        {
            PermissionResult::approve_once()
        } else {
            PermissionResult::reject(Some("The user did not approve this operation.".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        forbidden_permission_path, permission_summary, read_inside_workspace,
        requests_sandbox_escalation, shortened,
    };
    use github_copilot_sdk::types::{PermissionRequestData, PermissionRequestKind};
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    #[test]
    fn only_native_canvas_reads_skip_approval_and_never_managed_policy() {
        let mut data = request(PermissionRequestKind::CustomTool, "");
        data.extra = json!({"permissionRequest":{"toolName":"open_canvas"}});
        assert!(super::reads_without_asking(Path::new("/workspace"), &data));
        data.managed_approval_required = Some(true);
        assert!(!super::reads_without_asking(Path::new("/workspace"), &data));
        data.managed_approval_required = None;
        data.extra = json!({"permissionRequest":{"toolName":"invoke_canvas_action"}});
        assert!(!super::reads_without_asking(Path::new("/workspace"), &data));
    }

    fn request(kind: PermissionRequestKind, path: &str) -> PermissionRequestData {
        let mut data: PermissionRequestData = serde_json::from_value(json!({})).unwrap();
        data.kind = Some(kind);
        data.extra = json!({"permissionRequest": {"path": path}});
        data
    }

    #[test]
    fn a_read_is_silent_only_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let inside = workspace.join("attachments/reference.txt");
        fs::create_dir_all(inside.parent().unwrap()).unwrap();
        fs::write(&inside, "evidence").unwrap();
        let outside = dir.path().parent().unwrap().join("outside-read.txt");
        fs::write(&outside, "host file").unwrap();

        assert!(super::reads_without_asking(
            &workspace,
            &request(PermissionRequestKind::Read, &inside.to_string_lossy())
        ));
        assert!(!super::reads_without_asking(
            &workspace,
            &request(PermissionRequestKind::Read, &outside.to_string_lossy())
        ));
        // A path that does not exist cannot be proven to be inside.
        assert!(!super::reads_without_asking(
            &workspace,
            &request(
                PermissionRequestKind::Read,
                &workspace.join("missing.txt").to_string_lossy()
            )
        ));
        assert!(!super::reads_without_asking(
            &workspace,
            &request(PermissionRequestKind::Read, "")
        ));
        // Only reads are silent, whatever the path.
        assert!(!super::reads_without_asking(
            &workspace,
            &request(PermissionRequestKind::Shell, &inside.to_string_lossy())
        ));
        assert!(!super::reads_without_asking(
            &workspace,
            &request(PermissionRequestKind::Write, &inside.to_string_lossy())
        ));
        let mut managed = request(PermissionRequestKind::Read, &inside.to_string_lossy());
        managed.managed_approval_required = Some(true);
        assert!(!super::reads_without_asking(&workspace, &managed));
        let _ = fs::remove_file(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_workspace_is_not_an_inside_read() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let target = dir.path().join("host-secret.txt");
        fs::write(&target, "host file").unwrap();
        let link = workspace.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(!read_inside_workspace(
            &workspace,
            &json!({"path": link.to_string_lossy()})
        ));
        // The runtime's own canonical path is preferred when it is sent.
        assert!(!read_inside_workspace(
            &workspace,
            &json!({"resolvedPath": target.to_string_lossy(), "path": link.to_string_lossy()})
        ));
    }

    #[test]
    fn approvals_show_the_command_the_change_the_url_and_the_prompt() {
        let shell = permission_summary(&json!({
            "requestId": "r1",
            "permissionRequest": {"kind": "commands", "fullCommandText": "gh pr list --limit 5", "intention": "List PRs"}
        }));
        assert_eq!(
            shell,
            "Kind: commands\nWhy: List PRs\nCommand: gh pr list --limit 5"
        );
        let write = permission_summary(&json!({"permissionRequest": {
            "kind": "write", "fileName": "/w/notes.md", "intention": "Save notes",
            "diff": "@@\n+one\n+two", "newFileContents": "one\ntwo\n"
        }}));
        assert!(write.contains("File: /w/notes.md"));
        assert!(write.contains("Change:\n@@\n+one\n+two"));
        assert!(write.contains("New file: 8 characters, 2 lines"));
        let url = permission_summary(&json!({"permissionRequest": {
            "kind": "url", "url": "https://example.com/page", "redirectedFrom": "https://example.com"
        }}));
        assert!(url.contains("URL: https://example.com/page"));
        assert!(url.contains("Redirected from: https://example.com"));
        let task = permission_summary(&json!({"permissionRequest": {
            "kind": "customTool", "toolName": "task",
            "args": {"prompt": "Review the diff and report findings."}
        }}));
        assert!(task.contains("Tool: task"));
        assert!(task.contains("Prompt:\nReview the diff and report findings."));
        assert!(permission_summary(&json!({"odd": 1})).contains("\"odd\""));
    }

    #[test]
    fn long_changes_and_prompts_are_shortened_but_say_so() {
        let many = (0..40)
            .map(|line| format!("+line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let short = shortened(&many);
        assert!(short.contains("+line 19"));
        assert!(!short.contains("+line 20"));
        assert!(short.contains("… 20 more lines"));
        let wide = "x".repeat(5000);
        assert!(shortened(&wide).contains("shortened; 5000 characters in all"));
        assert_eq!(shortened("one\ntwo"), "one\ntwo");
    }

    #[test]
    fn an_ordinary_shell_command_is_offered_but_an_escalation_is_refused() {
        // The CLI sends these flags on every shell prompt, so the false case
        // must still reach the approval card.
        let ordinary = json!({"permissionRequest": {
            "kind": "commands", "fullCommandText": "gh pr list",
            "requestSandboxBypass": false, "requestSandboxPermissive": false
        }});
        assert!(!requests_sandbox_escalation(&ordinary));
        for escalation in [
            json!({"permissionRequest": {"kind": "commands", "requestSandboxBypass": true}}),
            json!({"permissionRequest": {"kind": "commands", "requestSandboxPermissive": true}}),
            json!({"permissionRequest": {"kind": "commands", "sandboxPathGrant": {"paths": ["/"]}}}),
            json!({"permissionRequest": {"kind": "sandbox-bypass"}}),
            json!({"permissionRequest": {"kind": "url", "url": "https://example.com", "requestSandboxBypass": true}}),
        ] {
            assert!(requests_sandbox_escalation(&escalation), "{escalation}");
        }
        assert!(!requests_sandbox_escalation(
            &json!({"permissionRequest": {"kind": "read", "path": "/w/notes.md"}})
        ));
    }

    #[test]
    fn credential_paths_are_denied_in_nested_permission_requests() {
        assert!(forbidden_permission_path(
            &json!({"paths":["/Users/work/.ssh/id_ed25519"]})
        ));
        assert!(forbidden_permission_path(
            &json!({"command":"cat ~/.config/subspace/copilot.json"})
        ));
        assert!(forbidden_permission_path(
            &json!({"path":r"C:\Users\work\.aws\credentials"})
        ));
        assert!(!forbidden_permission_path(
            &json!({"path":"/private/workspace/attachments/reference.txt"})
        ));
    }

    #[test]
    fn the_workspace_rule_is_a_prefix_of_whole_components() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("copilot-workspace");
        let sibling = dir.path().join("copilot-workspace-other");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        let file = sibling.join("notes.txt");
        fs::write(&file, "outside").unwrap();
        assert!(!read_inside_workspace(
            &workspace.canonicalize().unwrap(),
            &json!({"path": file.to_string_lossy()})
        ));
        assert!(!read_inside_workspace(
            Path::new("/no/such/workspace"),
            &json!({"path": file.to_string_lossy()})
        ));
    }
}
