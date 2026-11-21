use std::path::Path;

use crate::quote::python_string;

pub(crate) fn probe_script(canary: &Path, workspace: &Path, port: u16) -> String {
    render_probe_script(
        &python_string(&canary.to_string_lossy()),
        &python_string(&workspace.to_string_lossy()),
        port,
    )
}

fn render_probe_script(canary: &str, workspace: &str, port: u16) -> String {
    format!(
        r#"import errno, pathlib, socket
canary = pathlib.Path({canary})
workspace = pathlib.Path({workspace})
try:
    canary.read_text()
except OSError as error:
    if error.errno not in (errno.EPERM, errno.EACCES):
        raise RuntimeError(f"canary read failed with unexpected errno {{error.errno}}")
else:
    raise RuntimeError("canary read unexpectedly allowed")
try:
    canary.write_text("unexpected-write")
except OSError as error:
    if error.errno not in (errno.EPERM, errno.EACCES):
        raise RuntimeError(f"canary write failed with unexpected errno {{error.errno}}")
else:
    raise RuntimeError("canary write unexpectedly allowed")
workspace.joinpath("mxc-probe-workspace.txt").write_text("workspace-write-ok")
try:
    socket.create_connection(("127.0.0.1", {port}), 1)
except OSError as error:
    if error.errno not in (errno.EPERM, errno.EACCES):
        raise RuntimeError(f"loopback connect failed with unexpected errno {{error.errno}}")
else:
    raise RuntimeError("loopback connect unexpectedly allowed")
print("canary-read=denied canary-write=denied network=denied workspace-write=ok")
"#,
        canary = canary,
        workspace = workspace,
        port = port,
    )
}

#[cfg(test)]
mod tests {
    use super::probe_script;
    use std::path::Path;

    #[test]
    fn probe_script_is_synthetic_host_read_write_and_loopback() {
        let script = probe_script(Path::new("/tmp/canary.txt"), Path::new("/workspace"), 54321);
        assert!(script.contains("canary.read_text()"));
        assert!(script.contains("canary.write_text(\"unexpected-write\")"));
        assert!(script.contains("socket.create_connection((\"127.0.0.1\", 54321), 1)"));
        assert!(script.contains("mxc-probe-workspace.txt"));
        assert!(
            script.contains(
                "canary-read=denied canary-write=denied network=denied workspace-write=ok"
            )
        );
        assert!(!script.contains("/etc/passwd"));
        assert!(!script.contains("os.environ"));
    }
}
