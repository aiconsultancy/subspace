use crate::cli::{commands, run_catalog};
use crate::root::complete_word;
use crate::tree::Capabilities;
use crate::{approval::CliApproval, input};
use serde_json::{Value, json};
use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use subspace_agent_tools::{
    Access, ApprovalPolicy, Catalog, Invocation, ToolSpec, Unmonitored, WriteService, WriteTool,
    schema,
};
use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};

fn context() -> ProductContext {
    let product = fixture::products()
        .unwrap()
        .into_iter()
        .find(|product| product.id.as_str() == "subspace")
        .unwrap();
    ProductContext::new(
        product,
        PlatformEnv {
            os: OsKind::Linux,
            home: "/nonexistent-board-command-home".into(),
            xdg_config_home: None,
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        },
    )
}

struct Notes(Arc<AtomicUsize>);

impl WriteService for Notes {
    type Operation = Value;
    type Prepared = ();
    fn validate(&self, operations: &[Value]) -> Result<(), String> {
        if operations.iter().any(|op| op["body"].as_str().is_none()) {
            Err("body required".into())
        } else {
            Ok(())
        }
    }
    fn preflight(&self, operations: &[Value]) -> Result<(), String> {
        if operations.iter().any(|op| op["body"] == "fail") {
            Err("preflight refused".into())
        } else {
            Ok(())
        }
    }
    fn describe(&self, _: &[Value], _: &()) -> String {
        String::new()
    }
    fn write(&self, _: usize, operation: &Value, _: &()) -> Result<Value, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(operation.clone())
    }
}

/// Every tool in `catalog` as one a declared Space provides.
fn served(catalog: Catalog) -> Capabilities {
    Capabilities {
        space_tools: catalog
            .tools()
            .iter()
            .map(|tool| tool.spec().name)
            .collect(),
        catalog,
    }
}

fn catalog(count: &Arc<AtomicUsize>) -> Capabilities {
    let tool = WriteTool::new(
        ToolSpec {
            name: "actions_write",
            title: "Notes",
            description: "Fixture notes",
            access: Access::Write,
            approval: ApprovalPolicy::PerCall,
            arguments: schema::batch(&[schema::action(
                "actions_note",
                "Write a note",
                vec![("body", schema::text(), true)],
            )]),
        },
        Arc::new(Notes(count.clone())),
    );
    served(Catalog::new(vec![Arc::new(tool)]).unwrap())
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).into()).collect()
}

mod collisions;
mod composition;
mod fallback;
mod naming;
mod parse_errors;
mod product_flags;
mod reserved;
mod root;

#[test]
fn generated_help_completion_and_invalid_flags_use_the_catalog() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    let commands = commands(&context, &catalog).unwrap();
    assert_eq!(
        complete_word(&commands.spec, &strings(&["actions", ""])),
        ["note"]
    );
    assert!(
        complete_word(&commands.spec, &strings(&["actions", "note", "--b"]))
            .contains(&"--body".into())
    );
    assert_eq!(
        run_catalog(&context, &catalog, &strings(&["actions", "--help"])),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--unknown"])
        ),
        ExitCode::from(2)
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn unattended_write_refuses_and_yes_never_skips_preflight() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--body", "hello", "--json"])
        ),
        ExitCode::FAILURE
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--body", "fail", "--yes"])
        ),
        ExitCode::FAILURE
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--body", "hello", "--yes"])
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn batch_file_limits_and_flag_conflicts_are_parse_errors() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "[]").unwrap();
    let path = file.path().to_str().unwrap();
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--operations-file", path, "--yes"])
        ),
        ExitCode::from(2)
    );
    std::fs::write(
        file.path(),
        r#"[{"action":"actions_note","body":"one"},{"action":"actions_note","body":"two"}]"#,
    )
    .unwrap();
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&[
                "actions",
                "note",
                "--operations-file",
                path,
                "--body",
                "conflict",
                "--yes"
            ])
        ),
        ExitCode::from(2)
    );
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["actions", "note", "--operations-file", path, "--yes"])
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn oversized_batch_files_are_refused_before_any_write() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    let file = tempfile::NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap();
    let args = strings(&["actions", "note", "--operations-file", path, "--yes"]);
    let operations = vec![json!({"action":"actions_note","body":"bounded"}); 21];
    std::fs::write(file.path(), serde_json::to_vec(&operations).unwrap()).unwrap();
    assert_eq!(run_catalog(&context, &catalog, &args), ExitCode::from(2));
    std::fs::write(file.path(), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert_eq!(run_catalog(&context, &catalog, &args), ExitCode::from(2));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    std::fs::write(file.path(), serde_json::to_vec(&operations[..20]).unwrap()).unwrap();
    assert_eq!(run_catalog(&context, &catalog, &args), ExitCode::SUCCESS);
    assert_eq!(count.load(Ordering::SeqCst), 20);
}

#[tokio::test]
async fn cli_json_equals_mcp_structured_content_for_the_same_fixture() {
    use rmcp::ServiceExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let commands = commands(&context(), &catalog).unwrap();
    let parsed = usage::parse::Parser::new(&commands.spec)
        .parse(&strings(&[
            "subspace", "actions", "note", "--body", "fixture", "--yes", "--json",
        ]))
        .unwrap();
    let arguments = input::arguments(&commands.routes[0], &parsed).unwrap();
    let cli = commands.routes[0]
        .tool
        .call(Invocation {
            arguments: arguments.clone(),
            approval: Arc::new(CliApproval { yes: true }),
            monitor: Arc::new(Unmonitored),
        })
        .await
        .unwrap();
    let (client, server_end) = tokio::io::duplex(65536);
    let server = subspace_mcp::ToolServer::new(
        catalog.catalog,
        subspace_mcp::Identity::new("fixture", "Fixture", "1"),
    );
    let task = tokio::spawn(async move {
        server
            .serve(tokio::io::split(server_end))
            .await
            .unwrap()
            .waiting()
            .await
    });
    let (reader, mut writer) = tokio::io::split(client);
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28", "io.modelcontextprotocol/clientCapabilities":{}, "io.modelcontextprotocol/clientInfo":{"name":"fixture","version":"1"}},
        "name":"actions_write","arguments":arguments}});
    writer
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    let mut lines = BufReader::new(reader).lines();
    let line = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["result"]["structuredContent"], cli);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    task.abort();
}
