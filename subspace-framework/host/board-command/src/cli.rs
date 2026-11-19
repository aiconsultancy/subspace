use crate::{execute, fail, root, tree};
use std::process::ExitCode;
use subspace_agent_tools::{Catalog, ToolEnvironment};
use subspace_product::ProductContext;
use usage::parse::{ParseOutput, ParseValue};

/// Headless product command using the same served catalog as MCP.
pub fn run(
    context: &ProductContext,
    boards: &[subspace_mcp::Board],
    environment: &ToolEnvironment,
    args: &[String],
) -> ExitCode {
    match subspace_mcp::served_catalog(boards, &[], environment) {
        Ok(catalog) => run_catalog(context, &catalog, args),
        Err(error) => fail(&error, 1),
    }
}

pub(crate) fn commands(
    context: &ProductContext,
    catalog: &Catalog,
) -> Result<tree::Commands, String> {
    tree::Commands::new(context, catalog, root::tree)
}

pub(crate) fn run_catalog(
    context: &ProductContext,
    catalog: &Catalog,
    args: &[String],
) -> ExitCode {
    let generated = match commands(context, catalog) {
        Ok(generated) => generated,
        Err(error) => return fail(&error, 1),
    };
    if let Some(words) = args.strip_prefix(&[root::COMPLETE_WORD.into(), "--".into()]) {
        for word in root::complete_word(&generated.spec, words) {
            println!("{word}");
        }
        return ExitCode::SUCCESS;
    }
    if args.len() == 1
        && generated
            .routes
            .iter()
            .any(|route| route.path.len() == 2 && route.path[0] == args[0])
    {
        return parse_and_run(context, &generated, &[args[0].clone(), "--help".into()]);
    }
    parse_and_run(context, &generated, args)
}

fn parse_and_run(context: &ProductContext, commands: &tree::Commands, args: &[String]) -> ExitCode {
    let argv: Vec<String> = std::iter::once(commands.spec.bin.clone())
        .chain(args.iter().cloned())
        .collect();
    let output = match usage::parse::Parser::new(&commands.spec).explain(&argv) {
        Ok(output) => output,
        Err(error) => return fail(&error.to_string(), 2),
    };
    if let Some(page) = output.errors.iter().find(|error| {
        matches!(
            error,
            usage::error::UsageErr::Help(_) | usage::error::UsageErr::Version(_)
        )
    }) {
        print!("{page}");
        return ExitCode::SUCCESS;
    }
    if !output.errors.is_empty() {
        for error in &output.errors {
            eprintln!("{error}");
        }
        return ExitCode::from(2);
    }
    let path: Vec<&str> = output
        .cmds
        .iter()
        .skip(1)
        .map(|cmd| cmd.name.as_str())
        .collect();
    if path == [root::COMPLETION] {
        return completion(context, &output);
    }
    match commands.routes.iter().find(|route| {
        route
            .path
            .iter()
            .map(String::as_str)
            .eq(path.iter().copied())
    }) {
        Some(route) => execute(route, &output),
        None => fail(&format!("Unknown command: {}", args.join(" ")), 2),
    }
}

fn completion(context: &ProductContext, output: &ParseOutput) -> ExitCode {
    let shell = output.args.iter().find_map(|(arg, value)| match value {
        ParseValue::String(shell) if arg.name == "shell" => Some(shell.as_str()),
        _ => None,
    });
    match shell.and_then(|shell| root::init_script(context.product.binary_name.as_str(), shell)) {
        Some(script) => {
            print!("{script}");
            ExitCode::SUCCESS
        }
        None => fail("Choose a completion shell: bash, zsh or fish.", 2),
    }
}
