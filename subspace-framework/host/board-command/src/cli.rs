use crate::{Composition, execute, fail, root, tree};
use std::{collections::HashMap, process::ExitCode};
use subspace_agent_tools::ToolEnvironment;
use subspace_product::ProductContext;
use usage::parse::{ParseOutput, ParseValue, TokenRole};

/// Headless product command using the same served catalog as MCP.
pub fn run(
    context: &ProductContext,
    boards: &[subspace_mcp::Board],
    environment: &ToolEnvironment,
    args: &[String],
) -> ExitCode {
    match capabilities(context, boards, environment) {
        Ok(capabilities) => run_catalog(context, &capabilities, args),
        Err(error) => fail(&error, 1),
    }
}

/// The served catalogue, with the tools each declared Space provides.
pub(crate) fn capabilities(
    context: &ProductContext,
    boards: &[subspace_mcp::Board],
    environment: &ToolEnvironment,
) -> Result<tree::Capabilities, String> {
    subspace_mcp::served_catalog(boards, &[], environment)
        .map(|grouped| tree::Capabilities::new(context, &grouped))
}

/// The default CLI: the Framework's root tree, its completion, and no
/// product commands, so an unmatched root command is reported.
const STANDARD: Composition = Composition::new(root::tree, root::complete_word, unknown_command);

type Fallback = fn(&ProductContext, &[String]) -> ExitCode;

#[cfg(test)]
pub(crate) fn commands(
    context: &ProductContext,
    capabilities: &tree::Capabilities,
) -> Result<tree::Commands, String> {
    STANDARD.commands(context, capabilities)
}

pub(crate) fn run_catalog(
    context: &ProductContext,
    capabilities: &tree::Capabilities,
    args: &[String],
) -> ExitCode {
    STANDARD.run_catalog(context, capabilities, args)
}

pub(crate) fn run_commands(
    context: &ProductContext,
    generated: &tree::Commands,
    args: &[String],
    complete: fn(&usage::Spec, &[String]) -> Vec<String>,
    fallback: Fallback,
) -> ExitCode {
    if let Some(words) = args.strip_prefix(&[root::COMPLETE_WORD.into(), "--".into()]) {
        for word in complete(&generated.spec, words) {
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
        return parse_and_run(
            context,
            generated,
            &[args[0].clone(), "--help".into()],
            fallback,
        );
    }
    parse_and_run(context, generated, args, fallback)
}

fn parse_and_run(
    context: &ProductContext,
    commands: &tree::Commands,
    args: &[String],
    fallback: Fallback,
) -> ExitCode {
    let argv: Vec<String> = std::iter::once(commands.spec.bin.clone())
        .chain(args.iter().cloned())
        .collect();
    let output = match usage::parse::Parser::new(&commands.spec).explain(&argv) {
        Ok(output) => output,
        Err(error) => return refused(context, commands, &argv, &error, fallback),
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
    // An error the parser collected came from a declaration that took the
    // word, so it never names an unmatched root command.
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
    if path == [root::COMPLETION] && root::declares_framework_completion(&commands.spec) {
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
        None => fallback(context, args),
    }
}

/// A line the parser refused. It goes to the fallback only when usage-lib
/// stopped on an `unexpected word` at the root and everything before that
/// word is a complete, valid line of root flags on its own; any other
/// failure is reported.
///
/// usage-lib drops the errors it had collected when it stops on an
/// unexpected word, so the words before the stop are validated by parsing
/// them alone with the full parser, which reaches its end and reports every
/// error. Only the stop's position comes from a token trace.
fn refused(
    context: &ProductContext,
    commands: &tree::Commands,
    argv: &[String],
    error: &usage::miette::Error,
    fallback: Fallback,
) -> ExitCode {
    let stop = unexpected_word(error).and_then(|word| root_stop(&commands.spec, argv, &word));
    match stop.filter(|&stop| valid_root_prefix(&commands.spec, &argv[..stop])) {
        Some(_) => fallback(context, &argv[1..]),
        None => fail(&error.to_string(), 2),
    }
}

/// The word usage-lib's binding stop names. usage-lib raises that stop as an
/// untyped message rather than a `UsageErr` variant, so its text is all
/// there is to read; the trace must then stop at the same word.
fn unexpected_word(error: &usage::miette::Error) -> Option<String> {
    error
        .to_string()
        .strip_prefix("unexpected word: ")
        .map(str::to_owned)
}

/// Where in `argv` the parse stopped on `word`, a non-flag word nothing
/// took. usage-lib 7.3.0 gives the message (`Parser::explain`) and the token
/// trace (`Parser::explain_refused`) only from separate parses, so the
/// position comes from a second one that runs nothing: no mount discovery
/// and no `choices run=`. It binds as the first did up to the first mount;
/// at a mount it stops with no refused word, and where it binds further
/// than the first it stops at another word, so both are reported. A
/// declared `default_subcommand` takes every non-flag word, so with one no
/// stop is at the root.
fn root_stop(spec: &usage::Spec, argv: &[String], word: &str) -> Option<usize> {
    if spec.default_subcommand.is_some() || word.starts_with('-') {
        return None;
    }
    let tokens = usage::parse::Parser::new(spec)
        .with_mount_outputs(HashMap::new())
        .without_running_commands()
        .explain_refused(argv)
        .err()?;
    let stop = tokens.iter().rev().find(|token| {
        token
            .roles
            .iter()
            .any(|role| !matches!(role, TokenRole::Unread))
    })?;
    let refused = stop
        .roles
        .iter()
        .any(|role| matches!(role, TokenRole::Refused { .. }));
    (refused && stop.word == word).then_some(stop.index)
}

/// Whether `prefix` (the program name and the words before the stop) parses
/// on its own, with every check the parser makes, into the root command with
/// no argument bound and no `--` separator: then the stop was at the root
/// and every word before it was a valid root flag or flag value. Any error
/// disqualifies it, a `required` root flag the line omits included, since
/// the product declared that every line needs it.
fn valid_root_prefix(spec: &usage::Spec, prefix: &[String]) -> bool {
    usage::parse::Parser::new(spec)
        .explain(prefix)
        .is_ok_and(|output| {
            output.errors.is_empty()
                && output.cmds.len() == 1
                && output.args.is_empty()
                && !output.double_dash_seen
        })
}

pub(crate) fn unknown_command(_: &ProductContext, args: &[String]) -> ExitCode {
    fail(&format!("Unknown command: {}", args.join(" ")), 2)
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
