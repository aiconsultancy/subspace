//! Commands derived from the same capability catalog as MCP. No GUI initialization.

mod approval;
mod cli;
mod input;
mod root;
#[cfg(test)]
mod tests;
mod tree;

use std::{process::ExitCode, sync::Arc};
use subspace_agent_tools::{Invocation, Unmonitored};

pub use cli::run;

fn execute(route: &tree::Route, output: &usage::parse::ParseOutput) -> ExitCode {
    let arguments = match input::arguments(route, output) {
        Ok(arguments) => arguments,
        Err(error) => return fail(&error, 2),
    };
    let yes = input::switch(output, "yes");
    if route.tool.spec().access == subspace_agent_tools::Access::Write
        && !yes
        && !approval::interactive()
    {
        return fail(
            "Confirmation requires a TTY or explicit --yes; no operation was executed.",
            1,
        );
    }
    let invocation = Invocation {
        arguments,
        approval: Arc::new(approval::CliApproval { yes }),
        monitor: Arc::new(Unmonitored),
    };
    finish(call(route, invocation), input::switch(output, "json"))
}

fn call(route: &tree::Route, invocation: Invocation) -> Result<serde_json::Value, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(route.tool.call(invocation))
}

fn finish(result: Result<serde_json::Value, String>, json: bool) -> ExitCode {
    match result {
        Ok(value) => {
            if json {
                println!("{value}");
            } else {
                println!("{value:#}");
            }
            if failed(&value) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => fail(&error, 1),
    }
}

fn failed(value: &serde_json::Value) -> bool {
    value["approved"] == false
        || value["results"]
            .as_array()
            .is_some_and(|results| results.iter().any(|result| result["success"] == false))
}

fn fail(error: &str, code: u8) -> ExitCode {
    eprintln!("{error}");
    ExitCode::from(code)
}
