use super::{Notes, catalog, context, strings};
use crate::cli::{commands, run_catalog};
use crate::input;
use crate::root::complete_word;
use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use subspace_agent_tools::{
    Access, AgentTool, ApprovalPolicy, Catalog, ToolSpec, WriteTool, schema,
};
use subspace_product::{BoardDescriptor, ProductContext, fixture, validate_product};

#[test]
fn canonical_actions_have_clean_help_and_hidden_compatible_aliases() {
    let count = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&count);
    let context = context();
    let commands = commands(&context, &catalog).unwrap();
    let note = &commands.spec.cmd.subcommands["actions"].subcommands["note"];
    assert_eq!(note.help.as_deref(), Some("Write a note"));
    assert_eq!(note.hidden_aliases, ["actions_note"]);
    assert!(note.aliases.is_empty());
    for action in ["note", "actions_note"] {
        let parsed = usage::parse::Parser::new(&commands.spec)
            .parse(&strings(&[
                "subspace", "actions", action, "--body", "fixture", "--yes",
            ]))
            .unwrap();
        let arguments = input::arguments(&commands.routes[0], &parsed).unwrap();
        assert_eq!(arguments["operations"][0]["action"], "actions_note");
        assert_eq!(
            run_catalog(
                &context,
                &catalog,
                &strings(&["actions", action, "--body", "fixture", "--yes"])
            ),
            ExitCode::SUCCESS
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(complete_word(&commands.spec, &strings(&["actions", "actions_"])).is_empty());
}

fn tool(name: &'static str, actions: &[schema::Action]) -> Arc<dyn AgentTool> {
    Arc::new(WriteTool::new(
        ToolSpec {
            name,
            title: "Fixture",
            description: "Fixture",
            access: Access::Write,
            approval: ApprovalPolicy::PerCall,
            arguments: schema::batch(actions),
        },
        Arc::new(Notes(Arc::new(AtomicUsize::new(0)))),
    ))
}

#[test]
fn command_names_use_kebab_case_and_reject_normalization_collisions() {
    let catalog = Catalog::new(vec![tool(
        "component_gallery_write",
        &[
            schema::action("component_gallery_save_chart", "Save chart.", vec![]),
            schema::action("component-gallery_export_chart", "Export chart.", vec![]),
            schema::action("move_to_group", "Move group.", vec![]),
        ],
    )])
    .unwrap();
    let generated = commands(&context(), &catalog).unwrap();
    assert_eq!(
        complete_word(&generated.spec, &strings(&["component-gallery", ""])),
        ["export-chart", "move-to-group", "save-chart"]
    );
    let mut tools = catalog.tools().to_vec();
    tools.push(tool(
        "component_gallery_other",
        &[schema::action("save_chart", "Collision.", vec![])],
    ));
    let error = commands(&context(), &Catalog::new(tools).unwrap())
        .err()
        .unwrap();
    assert!(error.contains("component-gallery save-chart"), "{error}");
}

pub(super) fn single_tool(name: &'static str) -> Arc<dyn AgentTool> {
    Arc::new(WriteTool::new(
        ToolSpec {
            name,
            title: "Fixture",
            description: "Fixture",
            access: Access::Write,
            approval: ApprovalPolicy::PerCall,
            arguments: schema::single(vec![schema::field("action", schema::text(), true)]),
        },
        Arc::new(Notes(Arc::new(AtomicUsize::new(0)))),
    ))
}

fn mixed_catalog() -> Catalog {
    Catalog::new(vec![
        single_tool("space_lab_inspect"),
        single_tool("space_lab_edit"),
        single_tool("irc_fixture"),
        single_tool("unrelated_fixture"),
        tool(
            "space_lab_project_inspect",
            &[schema::action("list", "List", vec![])],
        ),
        tool(
            "space_lab_project_edit",
            &[schema::action("save", "Save", vec![])],
        ),
        tool(
            "actions_write",
            &[schema::action("actions_note", "Note", vec![])],
        ),
        tool(
            "github_read",
            &[schema::action("github_issue", "Issue", vec![])],
        ),
    ])
    .unwrap()
}

fn mixed_context() -> ProductContext {
    let mut descriptor = fixture::descriptor();
    descriptor.boards = ["actions", "github", "irc", "space-lab"]
        .into_iter()
        .map(|id| BoardDescriptor {
            id: id.into(),
            aliases: vec![],
            default_enabled: false,
        })
        .collect();
    let product = validate_product(&descriptor).unwrap();
    ProductContext::new(product, context().env)
}

#[test]
fn colliding_single_tools_keep_distinct_roots_and_board_actions() {
    let commands = commands(&mixed_context(), &mixed_catalog()).unwrap();
    assert!(commands.spec.cmd.subcommands.contains_key("irc"));
    assert!(
        commands
            .spec
            .cmd
            .subcommands
            .contains_key("unrelated_fixture")
    );
    for (root, alias) in [
        ("space-lab-inspect", "space_lab_inspect"),
        ("space-lab-edit", "space_lab_edit"),
    ] {
        let command = &commands.spec.cmd.subcommands[root];
        assert_eq!(command.hidden_aliases, [alias]);
        for name in [root, alias] {
            let parsed = usage::parse::Parser::new(&commands.spec)
                .parse(&strings(&["subspace", name, "--action", "fixture"]))
                .unwrap();
            let route = commands
                .routes
                .iter()
                .find(|route| route.path == [root])
                .unwrap();
            assert_eq!(
                input::arguments(route, &parsed).unwrap()["action"],
                "fixture"
            );
        }
    }
}

#[test]
fn mixed_tools_preserve_existing_action_paths_and_aliases() {
    let commands = commands(&mixed_context(), &mixed_catalog()).unwrap();
    for (root, action) in [
        ("space-lab", "list"),
        ("space-lab", "save"),
        ("actions", "note"),
        ("github", "issue"),
    ] {
        assert!(
            commands.spec.cmd.subcommands[root]
                .subcommands
                .contains_key(action)
        );
    }
    assert_eq!(
        commands.spec.cmd.subcommands["actions"].subcommands["note"].hidden_aliases,
        ["actions_note"]
    );
    assert_eq!(
        commands.spec.cmd.subcommands["github"].subcommands["issue"].hidden_aliases,
        ["github_issue"]
    );
}

#[test]
fn a_tool_belongs_to_the_longest_board_id_prefix() {
    let mut descriptor = fixture::descriptor();
    descriptor.boards = ["github", "github-copilot-agents"]
        .into_iter()
        .map(|id| BoardDescriptor {
            id: id.into(),
            aliases: vec![],
            default_enabled: true,
        })
        .collect();
    let product = validate_product(&descriptor).unwrap();
    let context = ProductContext::new(product, context().env);
    let catalog = Catalog::new(vec![
        tool(
            "github_copilot_agents_write",
            &[schema::action("github_agent_task", "Start a task.", vec![])],
        ),
        tool(
            "github_write",
            &[schema::action("github_comment", "Comment.", vec![])],
        ),
    ])
    .unwrap();
    let commands = commands(&context, &catalog).unwrap();
    let roots = &commands.spec.cmd.subcommands;
    assert!(
        roots["github-copilot-agents"]
            .subcommands
            .contains_key("github-agent-task")
    );
    assert!(roots["github"].subcommands.contains_key("comment"));
    assert!(
        !roots["github"]
            .subcommands
            .contains_key("github-agent-task")
    );
}
