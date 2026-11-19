//! Running credential command-line tools without exposing their output in errors.

use std::time::Duration;

mod process;
mod request;

use request::{CapturedOutput, RunnerRequest};

/// Runs a CLI, injectable so tests fake the external process boundary.
pub trait Runner: Send + Sync {
    /// Runs `program` from the `PATH` with `args` and without the `remove`
    /// variables, and returns stdout on success. Errors never include output.
    fn run(
        &self,
        program: &str,
        args: &[&str],
        remove: &[&str],
        timeout: Duration,
    ) -> Result<String, String>;
}

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        remove: &[&str],
        timeout: Duration,
    ) -> Result<String, String> {
        let output = process::capture(RunnerRequest::new(program, args, remove, timeout))?;
        successful_output(program, output)
    }
}

fn successful_output(program: &str, output: CapturedOutput) -> Result<String, String> {
    match output.exit_code {
        Some(0) => Ok(output.stdout),
        _ => Err(format!("{program} failed")),
    }
}

#[cfg(test)]
mod tests;
