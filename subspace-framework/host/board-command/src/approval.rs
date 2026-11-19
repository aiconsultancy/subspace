use std::io::{IsTerminal, Write};
use subspace_agent_tools::Approval;

pub(crate) struct CliApproval {
    pub yes: bool,
}

pub(crate) fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

#[async_trait::async_trait]
impl Approval for CliApproval {
    async fn approve(&self, summary: String) -> Result<bool, String> {
        if self.yes {
            return Ok(true);
        }
        if !interactive() {
            return Err("Confirmation requires a TTY or explicit --yes.".into());
        }
        eprintln!("{summary}");
        eprint!("Apply this request? [y/N] ");
        std::io::stderr()
            .flush()
            .map_err(|error| error.to_string())?;
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|error| error.to_string())?;
        Ok(matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))
    }
}
