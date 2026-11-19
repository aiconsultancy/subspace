use super::naming::single_tool;
use super::{catalog, context, strings};
use crate::cli::{commands, run_catalog};
use crate::root::{complete_word, init_script};
use std::{
    process::ExitCode,
    sync::{Arc, atomic::AtomicUsize},
};
use subspace_agent_tools::{AgentTool, Catalog, ToolEnvironment};

#[test]
fn the_root_declares_completion_mcp_and_upgrade_beside_board_routes() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let commands = commands(&context(), &catalog).unwrap();
    assert_eq!(commands.spec.bin, "subspace");
    for name in ["completion", "__complete-word", "mcp", "upgrade", "actions"] {
        assert!(commands.spec.cmd.subcommands.contains_key(name), "{name}");
    }
    assert_eq!(
        complete_word(&commands.spec, &strings(&[""])),
        ["actions", "completion", "mcp", "upgrade"]
    );
    assert_eq!(
        complete_word(&commands.spec, &strings(&["completion", "z"])),
        ["zsh"]
    );
}

#[test]
fn a_product_without_updates_has_no_upgrade_command() {
    let mut context = context();
    context.product.update = None;
    let commands = commands(&context, &Catalog::new(Vec::new()).unwrap()).unwrap();
    assert!(!commands.spec.cmd.subcommands.contains_key("upgrade"));
}

#[test]
fn a_generated_command_cannot_replace_a_root_command() {
    let catalog = Catalog::new(vec![single_tool("completion")]).unwrap();
    assert_eq!(
        commands(&context(), &catalog).err().unwrap(),
        "Generated command collides with existing command: completion"
    );
    assert_eq!(
        run_catalog(&context(), &catalog, &strings(&["--help"])),
        ExitCode::FAILURE
    );
}

#[test]
fn completion_scripts_call_the_hidden_route_and_unknown_routes_fail() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let context = context();
    for shell in ["bash", "zsh", "fish"] {
        let script = init_script("subspace", shell).unwrap();
        assert!(script.contains("subspace __complete-word --"), "{shell}");
        assert_eq!(
            run_catalog(&context, &catalog, &strings(&["completion", shell])),
            ExitCode::SUCCESS
        );
    }
    assert!(init_script("subspace", "powershell").is_none());
    for args in [
        strings(&["completion", "powershell"]),
        strings(&["unknown-root"]),
        strings(&["actions", "note", "--invalid-flag"]),
    ] {
        assert_eq!(
            run_catalog(&context, &catalog, &args),
            ExitCode::from(2),
            "{args:?}"
        );
    }
    assert_eq!(
        run_catalog(
            &context,
            &catalog,
            &strings(&["__complete-word", "--", "actions", ""])
        ),
        ExitCode::SUCCESS
    );
}

fn duplicate_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    vec![
        single_tool("duplicate_fixture"),
        single_tool("duplicate_fixture"),
    ]
}

#[test]
fn a_catalog_construction_failure_is_reported() {
    let boards = [subspace_mcp::Board {
        id: "fixture",
        title: "Fixture",
        description: "",
        help: "",
        help_images: &[],
        tools: duplicate_tools,
    }];
    let environment = subspace_agent_tools::test_support::environment(Default::default());
    assert_eq!(
        crate::run(&context(), &boards, &environment, &strings(&["--help"])),
        ExitCode::FAILURE
    );
}
