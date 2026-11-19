use super::{convert_agent, stage_agent_file};
use github_copilot_sdk::rpc::AgentInfo;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn agent(path: &Path) -> AgentInfo {
    AgentInfo {
        name: "custom".into(),
        display_name: "Custom assistant".into(),
        description: "An authored assistant".into(),
        path: Some(path.to_string_lossy().into_owned()),
        tools: Some(vec!["view".into()]),
        skills: Some(vec!["review".into()]),
        ..Default::default()
    }
}

#[test]
fn discovery_metadata_uses_the_staged_authored_prompt() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("custom.agent.md");
    let stage = directory.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(
        &source,
        "---\nname: custom\ndescription: An authored assistant\n---\nRead the requested evidence.\n",
    )
    .unwrap();
    stage_agent_file(&source, &stage).unwrap();
    for sdk_prompt in [None, Some("Not the staged prompt".into())] {
        let mut discovered = agent(&stage.join("custom.agent.md"));
        discovered.prompt = sdk_prompt;
        let config = convert_agent(discovered, &stage, true).unwrap().unwrap();
        assert_eq!(config.name, "custom");
        assert_eq!(config.prompt, "Read the requested evidence.\n");
        assert_eq!(config.display_name.as_deref(), Some("Custom assistant"));
        assert_eq!(config.tools, Some(vec!["view".into()]));
        assert_eq!(config.skills, Some(vec!["review".into()]));
    }
}

#[test]
fn prompt_bodies_support_plain_markdown_and_windows_front_matter() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("custom.agent.md");
    for content in [
        "Read the requested evidence.\n",
        "\u{feff}---\r\nname: custom\r\n---\r\nRead the requested evidence.\n",
    ] {
        fs::write(&file, content).unwrap();
        let config = convert_agent(agent(&file), directory.path(), false)
            .unwrap()
            .unwrap();
        assert_eq!(config.prompt, "Read the requested evidence.\n");
        assert_eq!(config.skills, Some(Vec::new()));
    }
}

#[test]
fn unrelated_paths_and_reserved_subspace_definitions_are_not_imported() {
    let directory = tempfile::tempdir().unwrap();
    let stage = directory.path().join("stage");
    fs::create_dir(&stage).unwrap();
    for file in [
        directory.path().join("outside.agent.md"),
        stage.join("../outside.agent.md"),
    ] {
        assert!(convert_agent(agent(&file), &stage, true).unwrap().is_none());
    }
    let mut reserved = agent(&stage.join("subspace.agent.md"));
    reserved.name = "subspace".into();
    assert!(convert_agent(reserved, &stage, true).unwrap().is_none());
}

#[test]
fn embedded_mcp_declarations_are_still_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut discovered = agent(&directory.path().join("custom.agent.md"));
    discovered.mcp_servers = Some(HashMap::from([("server".into(), json!({}))]));
    let error = convert_agent(discovered, directory.path(), true).unwrap_err();
    assert!(error.contains("Embedded integrations are blocked"));
}

#[cfg(unix)]
#[test]
fn staged_symbolic_links_are_not_read_as_agent_prompts() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("outside.md");
    let stage = directory.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(&source, "Not an approved staged prompt").unwrap();
    let link = stage.join("custom.agent.md");
    std::os::unix::fs::symlink(&source, &link).unwrap();
    let error = convert_agent(agent(&link), &stage, true).unwrap_err();
    assert!(error.contains("regular files"));
}
