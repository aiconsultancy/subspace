//! Board commands and `mcp`: start-up without GPUI.

use std::process::ExitCode;

use subspace_product::ProductContext;

use super::{CommandRunner, finish};
use crate::BoardRegistration;

/// `mcp`: the selected boards' agent tools for other harnesses, without GPUI
/// or a Copilot login.
/// `sign_in` is whether the app offers Sign in with GitHub, which guidance
/// may then name.
pub(super) fn headless(
    context: &ProductContext,
    (boards, sign_in): (&[BoardRegistration], bool),
    args: &[String],
    reranker: Option<crate::RerankerFactory>,
    command: CommandRunner,
) -> ExitCode {
    let files = crate::product::ProductFiles::for_context(context);
    let (mut environment, notices) = crate::product_tool_environment(
        context,
        &files,
        boards,
        subspace_agent_tools::ToolHost::Headless,
        sign_in,
    );
    let log_notice = crate::logging::start(&files, context);
    for notice in notices.into_iter().chain(log_notice) {
        eprintln!("{notice}");
    }
    let selected = match crate::registry::selected_registrations(
        &context.product,
        boards,
        &environment.config,
    ) {
        Ok(selected) => selected,
        Err(error) => return finish(Err(error)),
    };
    if let Err(error) =
        crate::search_runtime::initialize(&mut environment, context, &selected, &files, reranker)
    {
        eprintln!("Full-text search unavailable: {error}");
    }
    let boards = crate::registry::mcp_boards(&selected);
    if args.first().is_some_and(|arg| arg == "mcp") {
        subspace_mcp::run(
            context,
            &boards,
            &environment,
            &args[1..],
            &environment.config.mcp,
        )
    } else {
        command(context, &boards, &environment, args)
    }
}
