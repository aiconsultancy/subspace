use super::{catalog, context, served, strings};
use crate::{Composition, cli::commands, input, root, tree::Capabilities};
use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use subspace_agent_tools::{AgentTool, Catalog, ToolEnvironment};
use subspace_product::{BoardDescriptor, CommandMount, ProductContext, fixture, validate_product};

pub(super) fn root_context() -> ProductContext {
    root_context_for("actions")
}

pub(super) fn root_context_for(space: &str) -> ProductContext {
    let mut descriptor = fixture::descriptor();
    descriptor.id = "example-app".into();
    descriptor.binary_name = "example-app".into();
    descriptor.boards = vec![BoardDescriptor {
        id: space.into(),
        aliases: vec![],
        former_ids: vec![],
        default_enabled: true,
    }];
    descriptor.command_mount = CommandMount::Root;
    descriptor.update = None;
    ProductContext::new(validate_product(&descriptor).unwrap(), context().env)
}

fn product_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    let mut spec = root::tree(context)?;
    let extra: usage::Spec = r#"cmd "status" help="Product status" {
        flag "--format <FORMAT>" { choices "text" "json" }
    }"#
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())?;
    spec.cmd.subcommands.extend(extra.cmd.subcommands);
    Ok(spec)
}

fn fallback(_: &ProductContext, args: &[String]) -> ExitCode {
    assert!(
        args == strings(&["status", "--format", "json"]) || args == strings(&["external", "raw"])
    );
    ExitCode::from(7)
}

fn completion(spec: &usage::Spec, words: &[String]) -> Vec<String> {
    assert!(spec.cmd.subcommands.contains_key("status"));
    assert!(spec.cmd.subcommands.contains_key("note"));
    assert_eq!(words, strings(&["status", "--format", "j"]));
    root::complete_word(spec, words)
}

#[test]
fn root_routes_drop_the_space_prefix_but_keep_action_aliases_and_execution() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = root_context();
    let generated = commands(&context, &catalog).unwrap();
    assert_eq!(generated.routes[0].path, ["note"]);
    assert!(!generated.spec.cmd.subcommands.contains_key("actions"));
    assert_eq!(
        generated.spec.cmd.subcommands["note"].hidden_aliases,
        ["actions_note"]
    );
    assert_eq!(
        root::complete_word(&generated.spec, &strings(&["n"])),
        ["note"]
    );
    for name in ["note", "actions_note"] {
        let args = strings(&[name, "--body", "fixture", "--yes"]);
        let parsed = usage::parse::Parser::new(&generated.spec)
            .parse(&[strings(&["example-app"]), args.clone()].concat())
            .unwrap();
        assert_eq!(
            input::arguments(&generated.routes[0], &parsed).unwrap()["operations"][0]["action"],
            "actions_note"
        );
        assert_eq!(
            crate::cli::run_catalog(&context, &catalog, &args),
            ExitCode::SUCCESS
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn composition_merges_generated_routes_and_completes_the_product_tree() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let context = root_context();
    let composition = Composition::new(product_tree, completion, fallback);
    let generated = composition.commands(&context, &catalog).unwrap();
    for name in ["status", "note", "completion", "mcp"] {
        assert!(generated.spec.cmd.subcommands.contains_key(name), "{name}");
    }
    assert_eq!(
        composition.run_catalog(
            &context,
            &catalog,
            &strings(&["__complete-word", "--", "status", "--format", "j"])
        ),
        ExitCode::SUCCESS
    );
    assert_eq!(
        root::complete_word(&generated.spec, &strings(&["status", "--format", "j"])),
        ["json"]
    );
    assert_eq!(
        composition.run_catalog(&context, &catalog, &strings(&["--help"])),
        ExitCode::SUCCESS
    );
}

#[test]
fn fallback_receives_product_and_unknown_commands_but_not_invalid_generated_flags() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = root_context();
    let composition = Composition::new(product_tree, root::complete_word, fallback);
    for args in [
        strings(&["status", "--format", "json"]),
        strings(&["external", "raw"]),
    ] {
        assert_eq!(
            composition.run_catalog(&context, &catalog, &args),
            ExitCode::from(7)
        );
    }
    assert_eq!(
        composition.run_catalog(&context, &catalog, &strings(&["note", "--invalid"])),
        ExitCode::from(2)
    );
    assert_eq!(
        composition.run_catalog(
            &context,
            &catalog,
            &strings(&["status", "--format", "invalid"])
        ),
        ExitCode::from(2)
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn specialised_tools_are_excluded_only_from_generated_commands() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let composition = Composition::new(product_tree, root::complete_word, fallback)
        .with_specialized_tools(&["actions_write"]);
    let generated = composition.commands(&root_context(), &catalog).unwrap();
    assert!(generated.routes.is_empty());
    assert!(!generated.spec.cmd.subcommands.contains_key("note"));
    assert!(generated.spec.cmd.subcommands.contains_key("status"));
    assert_eq!(catalog.catalog.tools()[0].spec().name, "actions_write");
    assert_eq!(catalog.catalog.tools().len(), 1);
    assert!(
        Composition::new(product_tree, root::complete_word, fallback)
            .commands(&root_context(), &served(Catalog::new(vec![]).unwrap()))
            .is_ok()
    );
}

#[test]
fn root_single_tools_drop_the_space_prefix_and_shared_tools_keep_their_names() {
    let catalog = Capabilities {
        catalog: Catalog::new(vec![
            super::naming::single_tool("actions_inspect"),
            super::naming::single_tool("shared_fixture"),
            super::naming::single_tool("actions_shared"),
        ])
        .unwrap(),
        space_tools: ["actions_inspect"].into(),
    };
    let generated = commands(&root_context(), &catalog).unwrap();
    assert_eq!(generated.routes[2].path, ["actions_shared"]);
    assert_eq!(generated.routes[0].path, ["inspect"]);
    assert_eq!(generated.routes[1].path, ["shared_fixture"]);
    assert_eq!(
        generated.spec.cmd.subcommands["inspect"].hidden_aliases,
        ["actions_inspect"]
    );
    assert_eq!(
        root::complete_word(&generated.spec, &strings(&["i"])),
        ["inspect"]
    );
    assert!(
        generated
            .spec
            .cmd
            .subcommands
            .contains_key("shared_fixture")
    );
}

#[test]
fn without_a_catalogue_the_product_tree_still_parses_before_the_fallback() {
    let boards = [subspace_mcp::Board {
        id: "actions",
        title: "Fixture",
        description: "",
        help: "",
        help_images: &[],
        tools: |_| {
            vec![
                super::naming::single_tool("duplicate_fixture"),
                super::naming::single_tool("duplicate_fixture"),
            ]
        },
    }];
    let environment = subspace_agent_tools::test_support::environment(Default::default());
    let composition = Composition::new(product_tree, root::complete_word, fallback);
    for (args, code) in [
        (strings(&["external", "raw"]), 7),
        (strings(&["status", "--format", "json"]), 7),
        (strings(&["status", "--format", "invalid"]), 2),
        (strings(&["completion", "zsh"]), 0),
    ] {
        assert_eq!(
            composition.run(&root_context(), &boards, &environment, &args),
            ExitCode::from(code),
            "{args:?}"
        );
    }
    let (capabilities, notice) = crate::composition::available(Err("no tools".into()));
    assert!(capabilities.catalog.tools().is_empty());
    assert_eq!(
        notice.as_deref(),
        Some("Space commands unavailable: no tools")
    );
    assert!(
        crate::composition::available(Ok(Capabilities::default()))
            .1
            .is_none()
    );
}

#[test]
fn prefixed_compositions_keep_the_space_group_and_the_product_commands() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    let composition = Composition::new(product_tree, root::complete_word, fallback);
    let generated = composition.commands(&context, &catalog).unwrap();
    assert_eq!(generated.routes[0].path, ["actions", "note"]);
    for (args, code) in [
        (strings(&["actions", "note", "--body", "x", "--yes"]), 0),
        (strings(&["status", "--format", "json"]), 7),
        (strings(&["external", "raw"]), 7),
        (strings(&["actions", "note", "--invalid"]), 2),
    ] {
        assert_eq!(
            composition.run_catalog(&context, &catalog, &args),
            ExitCode::from(code),
            "{args:?}"
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

fn actions_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    catalog(&Arc::new(AtomicUsize::new(0)))
        .catalog
        .tools()
        .to_vec()
}

#[test]
fn the_standard_run_executes_root_mounted_space_actions() {
    let boards = [subspace_mcp::Board {
        id: "actions",
        title: "Actions",
        description: "",
        help: "",
        help_images: &[],
        tools: actions_tools,
    }];
    let environment = subspace_agent_tools::test_support::environment(Default::default());
    let run = |args: &[&str]| crate::run(&root_context(), &boards, &environment, &strings(args));
    assert_eq!(run(&["note", "--body", "x", "--yes"]), ExitCode::SUCCESS);
    assert_eq!(
        run(&["actions", "note", "--body", "x", "--yes"]),
        ExitCode::from(2)
    );
}
