use super::composition::root_context;
use super::{catalog, strings};
use crate::{Composition, root, tree::Capabilities};
use std::{
    process::ExitCode,
    sync::{Arc, atomic::AtomicUsize},
};
use subspace_product::ProductContext;

fn fallback(_: &ProductContext, _: &[String]) -> ExitCode {
    ExitCode::from(7)
}

fn own_completion_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\ncmd \"completion\" help=\"Product completion\" {{ arg \"<shell>\" }}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn composition_answers_the_framework_completion_the_root_tree_declares() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let context = root_context();
    let composition = Composition::new(root::tree, root::complete_word, fallback);
    assert_eq!(
        composition.run_catalog(&context, &catalog, &strings(&["completion", "zsh"])),
        ExitCode::SUCCESS
    );
    assert_eq!(
        composition.run_catalog(&context, &catalog, &strings(&["completion", "powershell"])),
        ExitCode::from(2)
    );
}

#[test]
fn a_product_completion_of_its_own_reaches_the_fallback() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let composition = Composition::new(own_completion_tree, root::complete_word, fallback);
    assert_eq!(
        composition.run_catalog(&root_context(), &catalog, &strings(&["completion", "zsh"])),
        ExitCode::from(7)
    );
}

#[test]
fn the_standard_cli_reports_an_unmatched_root_command() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let context = root_context();
    let args = strings(&["external", "raw"]);
    assert_eq!(
        crate::cli::unknown_command(&context, &args),
        ExitCode::from(2)
    );
    assert_eq!(
        crate::cli::run_catalog(&context, &catalog, &args),
        ExitCode::from(2)
    );
}

fn flagged_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    let mut spec = root::tree(context)?;
    let extra = r#"flag "--verbose" global=#true help="More output"
cmd "need" help="Needs a flag" {
    flag "--target <TARGET>" required=#true
}"#
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())?;
    spec.cmd.flags.extend(extra.cmd.flags);
    spec.cmd.subcommands.extend(extra.cmd.subcommands);
    Ok(spec)
}

fn default_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\ndefault_subcommand \"status\"\ncmd \"status\" help=\"Status\" {{ flag \"--json\" }}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn a_root_flag_before_an_unmatched_command_still_reaches_the_fallback() {
    let catalog = catalog(&Arc::new(AtomicUsize::new(0)));
    let composition = Composition::new(flagged_tree, root::complete_word, fallback);
    // `need` without its flag is refused after binding, in the output's
    // errors rather than as a parse failure.
    let spec = flagged_tree(&root_context()).unwrap();
    let need = usage::parse::Parser::new(&spec).explain(&strings(&["example-app", "need"]));
    assert!(need.is_ok_and(|output| !output.errors.is_empty()));
    for (args, code) in [
        (&["--verbose", "external"][..], 7),
        (&["--bogus", "external"], 2),
        (&["need"], 2),
        (&["need", "--target", "x"], 7),
    ] {
        assert_eq!(
            composition.run_catalog(&root_context(), &catalog, &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}

#[test]
fn words_a_default_subcommand_refuses_are_its_errors_not_unmatched_commands() {
    let catalog = Capabilities::default();
    let composition = Composition::new(default_tree, root::complete_word, fallback);
    for (args, code) in [(&["bogus"][..], 2), (&["--json"], 2), (&["status"], 7)] {
        assert_eq!(
            composition.run_catalog(&root_context(), &catalog, &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}
