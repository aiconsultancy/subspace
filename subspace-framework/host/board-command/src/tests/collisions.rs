use super::composition::{root_context, root_context_for};
use super::{catalog, served, strings};
use crate::{Composition, cli::commands, root, tree::Capabilities};
use std::{
    process::ExitCode,
    sync::{Arc, atomic::AtomicUsize},
};
use subspace_agent_tools::{AgentTool, Catalog, ToolEnvironment, ToolProvider, schema};
use subspace_product::{ProductContext, fixture};

fn fallback(_: &ProductContext, _: &[String]) -> ExitCode {
    ExitCode::from(7)
}

#[test]
fn root_routes_cannot_take_a_word_start_up_answers_first() {
    for (name, updates) in [
        ("actions_about", false),
        ("actions_mcp", false),
        ("actions_upgrade", true),
    ] {
        let mut context = root_context();
        if updates {
            context.product.update = fixture::descriptor().update;
        }
        let catalog = served(Catalog::new(vec![super::naming::single_tool(name)]).unwrap());
        let command = name.trim_start_matches("actions_");
        assert_eq!(
            commands(&context, &catalog).err().unwrap(),
            format!("Generated command collides with a reserved root word: {command}")
        );
    }
    let catalog =
        served(Catalog::new(vec![super::naming::single_tool("actions_upgrade")]).unwrap());
    let generated = commands(&root_context(), &catalog).unwrap();
    assert_eq!(generated.routes[0].path, ["upgrade"]);
}

#[test]
fn an_action_named_after_a_start_up_word_with_underscores_is_refused() {
    let catalog = served(
        Catalog::new(vec![super::naming::tool(
            "actions_write",
            &[schema::action("__product_metadata", "Fixture", vec![])],
        )])
        .unwrap(),
    );
    assert_eq!(
        commands(&root_context(), &catalog).err().unwrap(),
        "Generated command name reads as a flag: --product-metadata"
    );
}

#[test]
fn root_routes_cannot_replace_completion() {
    let catalog =
        served(Catalog::new(vec![super::naming::single_tool("actions_completion")]).unwrap());
    assert_eq!(
        commands(&root_context(), &catalog).err().unwrap(),
        "Generated command collides with existing command: completion"
    );
}

#[test]
fn group_roots_leaf_names_and_leaf_aliases_share_one_root_namespace() {
    let get = || vec![schema::action("get", "Get", vec![])];
    for (space, shared) in [
        (
            super::naming::single_tool("actions_shared"),
            super::naming::tool("shared", &get()),
        ),
        (
            super::naming::tool(
                "actions_write",
                &[schema::action("board_help", "Fixture", vec![])],
            ),
            super::naming::tool("board_help", &get()),
        ),
    ] {
        let name = shared.spec().name;
        let catalog = Capabilities {
            space_tools: [space.spec().name].into(),
            catalog: Catalog::new(vec![space, shared]).unwrap(),
        };
        assert_eq!(
            commands(&root_context(), &catalog).err().unwrap(),
            format!("Generated command collision: {name}")
        );
    }
}

fn looked_up_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    let spec = root::tree(context)?;
    assert!(spec.cmd.find_subcommand("completion").is_some());
    Ok(spec)
}

#[test]
fn generated_routes_parse_after_the_tree_has_looked_up_a_command() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let composition = Composition::new(looked_up_tree, root::complete_word, fallback);
    let generated = composition.commands(&root_context(), &catalog).unwrap();
    let parsed = usage::parse::Parser::new(&generated.spec)
        .parse(&strings(&["example-app", "actions_note", "--body", "x"]))
        .unwrap();
    assert_eq!(parsed.cmds.last().unwrap().name, "note");
}

fn alias_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\ncmd \"status\" {{ alias \"note\" }}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn generated_root_routes_cannot_replace_a_product_command_alias() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let composition = Composition::new(alias_tree, root::complete_word, fallback);
    assert_eq!(
        composition
            .commands(&root_context(), &catalog)
            .err()
            .unwrap(),
        "Generated command collides with existing command: note"
    );
}

fn web_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    vec![super::naming::tool(
        "web_notes",
        &[schema::action("read", "Read a note", vec![])],
    )]
}

/// Asks a Root-mounted product with the one Space `id` for each command's
/// help, through the standard run, and checks the exit codes.
fn assert_root_help(
    id: &'static str,
    help: &'static str,
    tools: ToolProvider,
    cases: &[(&str, u8)],
) {
    let boards = [subspace_mcp::Board {
        id,
        title: id,
        description: "",
        help,
        help_images: &[],
        tools,
    }];
    let environment = subspace_agent_tools::test_support::environment(Default::default());
    let context = root_context_for(id);
    for (command, code) in cases {
        assert_eq!(
            crate::run(
                &context,
                &boards,
                &environment,
                &strings(&[command, "--help"])
            ),
            ExitCode::from(*code),
            "{command}"
        );
    }
}

#[test]
fn only_the_spaces_own_tools_mount_at_the_root() {
    assert_root_help("web", "", web_tools, &[("read", 0), ("web_read", 0)]);
}

fn application_tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    vec![super::naming::tool(
        "application_notes",
        &[schema::action("application_list", "List notes", vec![])],
    )]
}

#[test]
fn a_space_named_application_does_not_inherit_the_shared_tools() {
    assert_root_help(
        "application",
        "# Application\n\nFixture notes.",
        application_tools,
        &[
            ("list", 0),
            ("board_help", 0),
            ("web_read", 0),
            ("web-read", 2),
        ],
    );
}
