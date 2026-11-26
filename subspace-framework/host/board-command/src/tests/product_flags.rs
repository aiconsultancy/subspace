use super::composition::root_context;
use super::{catalog, served, strings};
use crate::{Composition, root, tree::Capabilities};
use serde_json::Value;
use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use subspace_agent_tools::{
    Access, AgentTool, ApprovalPolicy, Catalog, Invocation, ToolSpec, schema,
};
use subspace_product::ProductContext;

fn fallback(_: &ProductContext, _: &[String]) -> ExitCode {
    ExitCode::from(7)
}

fn flagged(context: &ProductContext, flag: &str) -> Result<usage::Spec, String> {
    declared(
        context,
        &format!("flag {flag:?} global=#true help=\"Product flag\""),
    )
}

/// The Framework root tree with the product's root flag declaration `kdl`.
fn declared(context: &ProductContext, kdl: &str) -> Result<usage::Spec, String> {
    let mut spec = root::tree(context)?;
    let extra = kdl
        .parse::<usage::Spec>()
        .map_err(|error| error.to_string())?;
    spec.cmd.flags.extend(extra.cmd.flags);
    Ok(spec)
}

fn profile_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "--profile <NAME>")
}

#[test]
fn a_product_flag_never_reaches_a_generated_tool() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let composition = Composition::new(profile_tree, root::complete_word, fallback);
    for args in [
        &["--profile", "work", "note", "--body", "hello", "--yes"][..],
        &["note", "--body", "hello", "--profile", "work", "--yes"],
    ] {
        assert_eq!(
            composition.run_catalog(&root_context(), &catalog, &strings(args)),
            ExitCode::SUCCESS,
            "{args:?}"
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

fn body_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "--body <TEXT>")
}

fn yes_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "-y --yes")
}

fn renamed_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "body: --text <TEXT>")
}

fn assume_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "yes: --assume")
}

fn negated_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    declared(context, r#"flag "--pretty" negate="--json" global=#true"#)
}

type Tree = fn(&ProductContext) -> Result<usage::Spec, String>;

#[test]
fn a_product_flag_a_generated_command_also_declares_is_refused() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    for (tree, name) in [
        (body_tree as Tree, "--body"),
        (yes_tree, "--yes"),
        (renamed_tree, "body"),
        (assume_tree, "yes"),
        (negated_tree, "--json"),
    ] {
        let composition = Composition::new(tree, root::complete_word, fallback);
        assert_eq!(
            composition
                .commands(&root_context(), &catalog)
                .err()
                .unwrap(),
            format!("Generated command flag collides with a product flag: {name}")
        );
    }
}

fn operations_file_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "--operations-file <PATH>")
}

/// A non-batched read tool that counts its calls and echoes its input.
struct Jot(Arc<AtomicUsize>, ToolSpec);

#[async_trait::async_trait]
impl AgentTool for Jot {
    fn spec(&self) -> &ToolSpec {
        &self.1
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(invocation.arguments)
    }
}

fn single_note(count: &Arc<AtomicUsize>) -> Capabilities {
    let spec = ToolSpec {
        name: "actions_jot",
        title: "Jot",
        description: "Jot a note",
        access: Access::Read,
        approval: ApprovalPolicy::PerCall,
        arguments: schema::single(vec![schema::field("body", schema::text(), true)]),
    };
    served(Catalog::new(vec![Arc::new(Jot(count.clone(), spec))]).unwrap())
}

#[test]
fn a_product_operations_file_flag_never_reaches_a_route_without_batches() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = single_note(&count);
    let composition = Composition::new(operations_file_tree, root::complete_word, fallback);
    let args = [
        "jot",
        "--body",
        "hello",
        "--operations-file",
        "/nonexistent",
        "--yes",
    ];
    assert_eq!(
        composition.run_catalog(&root_context(), &catalog, &strings(&args)),
        ExitCode::SUCCESS
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

fn action_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    flagged(context, "--action <NAME>")
}

#[test]
fn a_product_action_flag_never_counts_as_an_operation_field() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let composition = Composition::new(action_tree, root::complete_word, fallback);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), r#"[{"action":"actions_note","body":"one"}]"#).unwrap();
    let path = file.path().to_str().unwrap();
    let args = [
        "note",
        "--action",
        "other",
        "--operations-file",
        path,
        "--yes",
    ];
    assert_eq!(
        composition.run_catalog(&root_context(), &catalog, &strings(&args)),
        ExitCode::SUCCESS
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
