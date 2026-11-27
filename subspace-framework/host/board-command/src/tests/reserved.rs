use super::composition::root_context;
use super::{context, served, strings};
use crate::{Composition, cli::commands, root};
use std::process::ExitCode;
use subspace_agent_tools::{Catalog, schema};
use subspace_product::{BoardDescriptor, ProductContext, fixture, validate_product};

fn fallback(_: &ProductContext, _: &[String]) -> ExitCode {
    ExitCode::from(7)
}

fn about_context() -> ProductContext {
    let mut descriptor = fixture::descriptor();
    descriptor.id = "example-app".into();
    descriptor.binary_name = "example-app".into();
    descriptor.boards = vec![BoardDescriptor {
        id: "about".into(),
        aliases: vec![],
        former_ids: vec![],
        default_enabled: true,
    }];
    ProductContext::new(validate_product(&descriptor).unwrap(), context().env)
}

#[test]
fn a_prefixed_space_may_be_named_after_a_word_start_up_claims_only_alone() {
    let catalog = served(
        Catalog::new(vec![super::naming::tool(
            "about_notes",
            &[schema::action("about_read", "Read a note", vec![])],
        )])
        .unwrap(),
    );
    let context = about_context();
    let generated = commands(&context, &catalog).unwrap();
    assert_eq!(generated.routes[0].path, ["about", "read"]);
    assert_eq!(
        crate::cli::run_catalog(&context, &catalog, &strings(&["about", "read", "--help"])),
        ExitCode::SUCCESS
    );
}

#[test]
fn no_space_may_take_a_word_start_up_claims_with_arguments() {
    for id in ["mcp", "upgrade", "__complete-word"] {
        let mut descriptor = fixture::descriptor();
        descriptor.boards[0].id = id.into();
        assert!(validate_product(&descriptor).is_err(), "{id}");
    }
}
fn bare_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!("bin {:?}", context.product.binary_name.as_str())
        .parse::<usage::Spec>()
        .map_err(|error| error.to_string())
}

#[test]
fn an_action_named_after_the_completion_word_the_tree_omits_is_refused() {
    let catalog = served(
        Catalog::new(vec![super::naming::tool(
            "actions_write",
            &[schema::action("__complete-word", "Fixture", vec![])],
        )])
        .unwrap(),
    );
    let composition = Composition::new(bare_tree, root::complete_word, fallback);
    assert_eq!(
        composition
            .commands(&root_context(), &catalog)
            .err()
            .unwrap(),
        "Generated command name reads as a flag: --complete-word"
    );
}

#[test]
fn a_generated_name_that_reads_as_a_flag_is_refused_at_any_depth() {
    let root_help =
        served(Catalog::new(vec![super::naming::single_tool("actions___help")]).unwrap());
    let error = "Generated command name reads as a flag: --help";
    assert_eq!(commands(&root_context(), &root_help).err().unwrap(), error);
    assert_eq!(
        crate::cli::run_catalog(&root_context(), &root_help, &strings(&["--help"])),
        ExitCode::FAILURE
    );
    let nested = served(
        Catalog::new(vec![super::naming::tool(
            "actions_write",
            &[schema::action("actions___help", "Fixture", vec![])],
        )])
        .unwrap(),
    );
    assert_eq!(commands(&context(), &nested).err().unwrap(), error);
}

#[test]
fn a_shared_tool_named_after_the_completion_word_is_refused() {
    let catalog =
        served(Catalog::new(vec![super::naming::single_tool("__complete-word")]).unwrap());
    assert_eq!(
        commands(&context(), &catalog).err().unwrap(),
        "Generated command collides with a reserved root word: __complete-word"
    );
}
