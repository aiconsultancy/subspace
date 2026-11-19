//! Signing the active gh account in again with `gh auth refresh`. Without a
//! terminal, gh prints a one-time code and waits until the user enters it on
//! [`DEVICE_URL`] or the code expires. gh refreshes only its active account.

use std::io::{BufRead, BufReader, Lines};
use std::process::{Child, ChildStderr, Command, Stdio};

use super::CredentialError;

/// Where the user enters the one-time code.
pub const DEVICE_URL: &str = "https://github.com/login/device";
/// The longest gh message a failure repeats.
const MAX_REASON: usize = 300;

/// A running `gh auth refresh` that printed its one-time code.
pub struct GhSignIn {
    child: Child,
    stderr: Lines<BufReader<ChildStderr>>,
    code: String,
}

impl GhSignIn {
    /// Starts `gh auth refresh` for github.com and waits for its one-time code.
    pub fn start() -> Result<Self, CredentialError> {
        let gh = crate::path::find_tool("gh")
            .map_err(|_| failed("gh is not on the PATH. Install the GitHub CLI"))?;
        let mut child = Command::new(gh)
            .args(["auth", "refresh", "--hostname", "github.com"])
            // gh refuses to refresh while one of these overrides its accounts.
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| failed("cannot start gh"))?;
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            return Err(failed("gh has no output"));
        };
        let mut stderr = BufReader::new(stderr).lines();
        match read_code(&mut stderr) {
            Ok(code) => Ok(Self {
                child,
                stderr,
                code,
            }),
            Err(reason) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(failed(&reason))
            }
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    /// Waits until gh exits: when GitHub confirms the code, or when the code
    /// expires after 15 minutes.
    pub fn finish(mut self) -> Result<(), CredentialError> {
        let reason = last_line(&mut self.stderr);
        match self.child.wait() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(failed(&reason.unwrap_or_else(|| "gh failed".into()))),
        }
    }
}

fn failed(reason: &str) -> CredentialError {
    CredentialError(format!("GitHub sign-in did not finish: {reason}."))
}

/// Reads gh's output up to its one-time code. Without a code, the reason is
/// gh's last line, such as an error for an account it cannot refresh.
fn read_code(lines: &mut impl Iterator<Item = std::io::Result<String>>) -> Result<String, String> {
    let mut last = None;
    for line in lines.map_while(Result::ok) {
        if let Some(code) = one_time_code(&line) {
            return Ok(code);
        }
        last = reason(&line).or(last);
    }
    Err(last.unwrap_or_else(|| "gh printed no one-time code".into()))
}

/// The rest of gh's output, which only has text when it failed.
fn last_line(lines: &mut impl Iterator<Item = std::io::Result<String>>) -> Option<String> {
    lines
        .map_while(Result::ok)
        .fold(None, |last, line| reason(&line).or(last))
}

/// A line of gh output as a short reason, without its decoration.
fn reason(line: &str) -> Option<String> {
    let text = line.trim();
    let text = ["! ", "X ", "✓ "]
        .iter()
        .find_map(|mark| text.strip_prefix(mark))
        .unwrap_or(text)
        .trim()
        .trim_end_matches('.');
    (!text.is_empty()).then(|| text.chars().take(MAX_REASON).collect())
}

/// The `XXXX-XXXX` device code in a line of gh output. gh prints either
/// "! First copy your one-time code: XXXX-XXXX" or
/// "! One-time code (XXXX-XXXX) copied to clipboard".
pub fn one_time_code(line: &str) -> Option<String> {
    if !line.to_ascii_lowercase().contains("one-time code") {
        return None;
    }
    line.split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .find(|word| is_device_code(word))
        .map(str::to_owned)
}

/// A device code is two groups of four characters joined by a dash.
const DEVICE_CODE_LENGTH: usize = 9;
const DEVICE_CODE_DASH: usize = 4;

fn is_device_code(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == DEVICE_CODE_LENGTH
        && bytes[DEVICE_CODE_DASH] == b'-'
        && bytes.iter().enumerate().all(|(index, byte)| {
            index == DEVICE_CODE_DASH || byte.is_ascii_uppercase() || byte.is_ascii_digit()
        })
}

#[cfg(test)]
mod tests {
    use super::{last_line, one_time_code, read_code};

    fn lines(text: &str) -> impl Iterator<Item = std::io::Result<String>> + '_ {
        text.lines().map(|line| Ok(line.to_owned()))
    }

    #[test]
    fn both_gh_code_prompts_give_the_code() {
        assert_eq!(
            one_time_code("! First copy your one-time code: FD30-E021").as_deref(),
            Some("FD30-E021")
        );
        assert_eq!(
            one_time_code("! One-time code (AB12-CD34) copied to clipboard").as_deref(),
            Some("AB12-CD34")
        );
        assert_eq!(
            one_time_code(
                "Open this URL to continue in your web browser: https://github.com/login/device"
            ),
            None
        );
        assert_eq!(one_time_code("! one-time code: not-a-code"), None);
    }

    #[test]
    fn a_failure_before_the_code_reports_gh_s_last_line() {
        let mut output = lines("\nX error refreshing credentials for octo-dev.\n");
        assert_eq!(
            read_code(&mut output).unwrap_err(),
            "error refreshing credentials for octo-dev"
        );
        let mut silent = lines("");
        assert!(
            read_code(&mut silent)
                .unwrap_err()
                .contains("no one-time code")
        );
    }

    #[test]
    fn the_code_is_read_and_the_rest_waits_for_the_exit() {
        let mut output = lines(
            "! One-time code (AB12-CD34) copied to clipboard\nOpen this URL to continue in your web browser: https://github.com/login/device\n✓ Authentication complete.\n",
        );
        assert_eq!(read_code(&mut output).as_deref(), Ok("AB12-CD34"));
        assert_eq!(
            last_line(&mut output).as_deref(),
            Some("Authentication complete")
        );
    }
}
