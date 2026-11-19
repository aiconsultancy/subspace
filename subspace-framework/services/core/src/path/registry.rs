//! The Windows registry `Path` and `%NAME%` expansion. The text functions
//! build on every platform so their tests run everywhere.

/// The value of the `Path` line in `reg query KEY /v Path` output. Registry
/// value names are case-insensitive, so `path` and `PATH` match too.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn parse_reg_query(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let name = line.split_whitespace().next()?;
        if !name.eq_ignore_ascii_case("Path") {
            return None;
        }
        let rest = line[name.len()..].trim_start();
        let kind = rest.split_whitespace().next()?;
        if kind != "REG_EXPAND_SZ" && kind != "REG_SZ" {
            return None;
        }
        Some(rest[kind.len()..].trim().to_string())
    })
}

/// Expand `%NAME%` references; unknown names stay as written.
pub(super) fn expand_percent(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(found) => out.push_str(&found),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The machine `Path` then the user `Path`, as a new Command Prompt builds
/// it, with `%VARIABLE%` references expanded. `reg.exe` gets `timeout`.
#[cfg(windows)]
pub(super) fn platform_path(
    timeout: std::time::Duration,
) -> Result<super::resolve::Found, super::resolve::CaptureError> {
    use std::ffi::OsString;

    use super::Capture;
    use super::resolve::{Found, Source};

    const MACHINE: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    const USER: &str = r"HKCU\Environment";
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let reg = std::path::Path::new(&root).join("System32").join("reg.exe");
    let mut parts = Vec::new();
    for key in [MACHINE, USER] {
        let stdout = query(&reg, key, timeout)
            .map_err(|error| registry_error(format!("reg query {key} failed: {error}")))?;
        if let Some(value) = parse_reg_query(&stdout) {
            parts.push(expand_percent(&value, |name| std::env::var(name).ok()));
        }
    }
    if parts.is_empty() {
        return Err(registry_error(
            "the registry holds no Path value".to_string(),
        ));
    }
    // A Windows desktop app already inherits the user's environment
    // variables, so only Path needs the registry.
    Ok(Found {
        path: OsString::from(parts.join(";")),
        source: Source::Registry,
        capture: Capture::Registry,
        network: Vec::new(),
    })
}

/// How often a running `reg query` is checked against its deadline.
#[cfg(windows)]
const REG_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(20);

/// The standard output of `reg query KEY /v Path`, within `timeout`.
#[cfg(windows)]
fn query(reg: &std::path::Path, key: &str, timeout: std::time::Duration) -> Result<String, String> {
    wait_for_output(start_query(reg, key)?, timeout)
}

#[cfg(windows)]
fn start_query(reg: &std::path::Path, key: &str) -> Result<std::process::Child, String> {
    use std::os::windows::process::CommandExt as _;
    use std::process::{Command, Stdio};

    /// `CREATE_NO_WINDOW`: no console window flashes up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new(reg)
        .args(["query", key, "/v", "Path"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| error.kind().to_string())
}

#[cfg(windows)]
fn wait_for_output(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<String, String> {
    let deadline = std::time::Instant::now() + timeout;
    while child
        .try_wait()
        .map_err(|error| error.kind().to_string())?
        .is_none()
    {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("no answer within {} s", timeout.as_secs()));
        }
        std::thread::sleep(REG_POLL_INTERVAL);
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.kind().to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A registry failure; its reason holds key names, never values.
#[cfg(windows)]
fn registry_error(reason: String) -> super::resolve::CaptureError {
    super::resolve::CaptureError {
        capture: super::Capture::Other {
            reason: reason.clone(),
        },
        message: reason,
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_percent, parse_reg_query};

    #[test]
    fn reg_query_value_and_percent_expansion() {
        let output = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\bin;C:\\Tools\r\n\r\n";
        let value = parse_reg_query(output).unwrap();
        assert_eq!(value, r"%USERPROFILE%\bin;C:\Tools");
        let expanded = expand_percent(&value, |name| {
            (name == "USERPROFILE").then(|| r"C:\Users\fixture".to_string())
        });
        assert_eq!(expanded, r"C:\Users\fixture\bin;C:\Tools");
        assert_eq!(expand_percent("%UNKNOWN%;50%", |_| None), "%UNKNOWN%;50%");
    }

    #[test]
    fn reg_query_value_name_is_case_insensitive() {
        for name in ["path", "PATH", "PaTh"] {
            let output = format!(
                "HKEY_CURRENT_USER\\Environment\r\n    {name}    REG_SZ    C:\\Program Files\\Tool\r\n"
            );
            assert_eq!(
                parse_reg_query(&output).as_deref(),
                Some(r"C:\Program Files\Tool")
            );
        }
        assert_eq!(parse_reg_query("    Paths    REG_SZ    x\r\n"), None);
        assert_eq!(parse_reg_query("    Path    REG_DWORD    0x1\r\n"), None);
    }
}
